//! On-device turns. The model speaks freely. A JSON object in the reply can
//! still call a phone tool. Constrained grammars are not applied: Qwen's
//! combined `{"` piece made llama.cpp throw and abort the process.
//! Phone actions are executed by the caller, which uses `actions::run`.

use serde_json::{Value, json};

use crate::actions;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatTurn {
    pub role: &'static str,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Decision {
    pub say: String,
    pub tool: Option<String>,
    pub args: Value,
}

pub fn tool_names() -> Vec<String> {
    let mut names: Vec<String> = actions::tool_definitions()
        .as_array()
        .map(|list| list.iter().filter_map(|tool| tool["name"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    names.extend(crate::local_memory::tool_names());
    names
}

pub fn system_prompt(custom: &str, memory: &str, local_memory: &str, speech: &str) -> String {
    let custom: String = custom.trim().chars().take(500).collect();
    let mut tools = String::new();
    let mut listed = actions::tool_definitions().as_array().cloned().unwrap_or_default();
    listed.extend(crate::local_memory::tool_definitions());
    for tool in &listed {
        let name = tool["name"].as_str().unwrap_or("");
        let desc = brief_tool(tool["description"].as_str().unwrap_or(""));
        tools.push_str(&format!("- {name}: {desc}\n"));
    }
    let speech_line = if speech == "de" {
        "The user is speaking German. Put the spoken reply in the say field in German. JSON keys and tool names stay in English. This session is not using the English Moonshine model.\n"
    } else {
        "This session uses the English Moonshine Base speech model. It does not understand German.\n"
    };
    format!(
        "{custom}\n\nYou are running on this phone. Reply with one JSON object and nothing else, shaped as {{\"say\":\"short spoken reply\",\"tool\":null,\"args\":{{}}}}. say is what you speak. tool is null or one name from the list. args is that tool's arguments. Call the tool before saying a note, reminder, alarm, timer, calendar change, or pinned fact is done. remember_fact and forget_fact change on-device memory only. For open_browser, open_youtube, open_whatsapp, open_signal, open_email, open_call, open_sms, and share_item, say what will open and pass confirmed false until the user clearly agrees, then call the same tool with confirmed true. Nothing is sent, and a call is not placed, until they confirm in the other app. media_control, set_volume, set_dnd, go_home, read_notifications, and phone_status act on this phone. Times are local YYYY-MM-DDTHH:MM. If web_search returns an error, speak that error. When they ask what is saved, call list_notes, list_reminders, list_calendar_events, or list_people once. A range uses from and to, month, or span. Do not look up one day at a time.\n{speech_line}\nTools:\n{tools}\n{}\n{}"
    , clip_chars(memory.trim(), MEMORY_CLIP_CHARS), clip_chars(local_memory.trim(), MEMORY_CLIP_CHARS))
}

const TOOL_BLURB_CHARS: usize = 72;
const MEMORY_CLIP_CHARS: usize = 600;
const HISTORY_CHAR_BUDGET: usize = 1600;
const HISTORY_TURN_CHARS: usize = 500;
/// The turn the user just sent. Older turns stay short so the 2048 context still prefills.
const CURRENT_TURN_CHARS: usize = 8_000;

/// First sentence, capped. The long tool essays were most of the prefill.
fn brief_tool(desc: &str) -> String {
    let desc = desc.trim();
    if desc.is_empty() {
        return String::new();
    }
    let cut = desc.find('.').map(|i| i + 1).unwrap_or(desc.len());
    let sentence = &desc[..cut];
    let mut out: String = sentence.chars().take(TOOL_BLURB_CHARS).collect();
    if sentence.chars().count() > TOOL_BLURB_CHARS {
        if let Some(space) = out.rfind(' ') {
            if space >= 24 {
                out.truncate(space);
            }
        }
    }
    out.trim().trim_end_matches(',').trim().to_string()
}

/// ChatML, Gemma, and Llama 3 end a turn with different markers. Drop the
/// marker so it is not spoken and does not sit inside the JSON object.
fn strip_turn_markers(raw: &str) -> String {
    let mut end = raw.len();
    for marker in ["<|im_end|>", "<end_of_turn>", "<|eot_id|>"] {
        if let Some(at) = raw.find(marker) {
            end = end.min(at);
        }
    }
    raw[..end].to_string()
}

fn turn_body(history: &[ChatTurn], index: usize) -> String {
    let last = history.len().saturating_sub(1);
    let max = if index == last { CURRENT_TURN_CHARS } else { HISTORY_TURN_CHARS };
    clip_chars(history[index].text.trim(), max)
}

fn clip_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn prompt(system: &str, history: &[ChatTurn]) -> String {
    prompt_for("chatml", system, history)
}

/// ChatML for SmolLM and Qwen, Gemma's turn markers, or Llama 3 headers.
/// The family comes from the pack, or from the GGUF chat template for a custom model.
pub fn prompt_for(family: &str, system: &str, history: &[ChatTurn]) -> String {
    match family {
        "gemma" => prompt_gemma(system, history),
        "llama3" => prompt_llama3(system, history),
        _ => prompt_chatml(system, history),
    }
}

fn prompt_chatml(system: &str, history: &[ChatTurn]) -> String {
    let mut out = String::new();
    out.push_str("<|im_start|>system\n");
    out.push_str(system.trim());
    out.push_str("<|im_end|>\n");
    for (index, turn) in history.iter().enumerate() {
        let role = if turn.role == "assistant" { "assistant" } else { "user" };
        out.push_str("<|im_start|>");
        out.push_str(role);
        out.push('\n');
        out.push_str(&turn_body(history, index));
        out.push_str("<|im_end|>\n");
    }
    out.push_str("<|im_start|>assistant\n");
    out
}

fn prompt_gemma(system: &str, history: &[ChatTurn]) -> String {
    let mut out = String::from("<bos>");
    let mut first_user = true;
    for (index, turn) in history.iter().enumerate() {
        if turn.role == "assistant" {
            out.push_str("<start_of_turn>model\n");
            out.push_str(&turn_body(history, index));
            out.push_str("<end_of_turn>\n");
            continue;
        }
        out.push_str("<start_of_turn>user\n");
        if first_user {
            let system = system.trim();
            if !system.is_empty() {
                out.push_str(system);
                out.push_str("\n\n");
            }
            first_user = false;
        }
        out.push_str(&turn_body(history, index));
        out.push_str("<end_of_turn>\n");
    }
    out.push_str("<start_of_turn>model\n");
    out
}

fn prompt_llama3(system: &str, history: &[ChatTurn]) -> String {
    let mut out = String::from("<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n");
    out.push_str(system.trim());
    out.push_str("<|eot_id|>");
    for (index, turn) in history.iter().enumerate() {
        let role = if turn.role == "assistant" { "assistant" } else { "user" };
        out.push_str("<|start_header_id|>");
        out.push_str(role);
        out.push_str("<|end_header_id|>\n\n");
        out.push_str(&turn_body(history, index));
        out.push_str("<|eot_id|>");
    }
    out.push_str("<|start_header_id|>assistant<|end_header_id|>\n\n");
    out
}

pub fn parse_decision(raw: &str, allowed: &[String]) -> Result<Decision, String> {
    let json_text = extract_json(raw);
    let value: Value = serde_json::from_str(json_text).map_err(|_| "the reply was not a JSON object".to_string())?;
    if !value.is_object() {
        return Err("the reply was not a JSON object".into());
    }
    let say = value["say"].as_str().unwrap_or("").trim().to_string();
    let tool = match value.get("tool") {
        None | Some(Value::Null) => None,
        Some(Value::String(name)) if name.trim().is_empty() || name == "null" => None,
        Some(Value::String(name)) => {
            let name = name.trim();
            if !allowed.iter().any(|allowed| allowed == name) {
                return Err(format!("{name} is not a phone tool"));
            }
            Some(name.to_string())
        }
        Some(_) => return Err("tool must be a name or null".into()),
    };
    let args = match value.get("args") {
        Some(Value::Object(_)) => value["args"].clone(),
        _ => json!({}),
    };
    Ok(Decision { say, tool, args })
}

fn extract_json(raw: &str) -> &str {
    let trimmed = raw.trim();
    let Some(start) = trimmed.find('{') else {
        return trimmed;
    };
    let Some(end) = trimmed.rfind('}') else {
        return trimmed;
    };
    if end < start {
        return trimmed;
    }
    &trimmed[start..=end]
}

pub fn trim_history(history: &mut Vec<ChatTurn>) {
    let mut total: usize = history.iter().map(|turn| turn.text.len()).sum();
    while history.len() > 2 && total > HISTORY_CHAR_BUDGET {
        let removed = history.remove(0);
        total = total.saturating_sub(removed.text.len());
    }
}

const SPOKEN_FALLBACK: &str = "Please say that again.";

/// A llama.cpp grammar or sampler failure. Rust `catch_unwind` does not see it;
/// the native boundary turns it into this kind of error so the turn can continue.
pub fn is_grammar_engine_failure(err: &str) -> bool {
    let err = err.to_ascii_lowercase();
    err.contains("grammar stack") || err.contains("grammar failed") || err.contains("grammar rejected")
}

fn finish_say(history: &mut Vec<ChatTurn>, last_say: &str) -> Result<String, String> {
    let say = if last_say.trim().is_empty() { SPOKEN_FALLBACK.to_string() } else { last_say.to_string() };
    history.push(ChatTurn { role: "assistant", text: say.clone() });
    Ok(say)
}

fn speakable(raw: &str, last_say: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return if last_say.trim().is_empty() { SPOKEN_FALLBACK.to_string() } else { last_say.to_string() };
    }
    trimmed.chars().take(280).collect()
}

fn looks_like_broken_json(raw: &str) -> bool {
    let trimmed = raw.trim();
    trimmed.starts_with('{') || trimmed.starts_with('[') || trimmed.starts_with("```")
}

/// One user turn. `complete` sees the chat prompt and an empty grammar.
/// `call_tool` runs the phone action.
#[cfg_attr(not(test), allow(dead_code))]
pub fn run_turns(
    system: &str,
    history: &mut Vec<ChatTurn>,
    user_text: &str,
    complete: impl FnMut(&str, &str) -> Result<String, String>,
    call_tool: impl FnMut(&str, &Value) -> String,
) -> Result<String, String> {
    run_turns_with(system, history, user_text, "chatml", complete, call_tool)
}

pub fn run_turns_with(
    system: &str,
    history: &mut Vec<ChatTurn>,
    user_text: &str,
    family: &str,
    mut complete: impl FnMut(&str, &str) -> Result<String, String>,
    mut call_tool: impl FnMut(&str, &Value) -> String,
) -> Result<String, String> {
    let names = tool_names();
    history.push(ChatTurn { role: "user", text: user_text.trim().to_string() });
    let mut repaired = false;
    let mut retried_engine = false;
    let mut last_say = String::new();
    for _ in 0..4 {
        trim_history(history);
        let prompt = prompt_for(family, system, history);
        let raw = match complete(&prompt, "") {
            Ok(raw) => strip_turn_markers(&raw),
            Err(err) if is_grammar_engine_failure(&err) => {
                log::warn!("on-device grammar failed ({err}); continuing this turn without a grammar");
                if retried_engine {
                    return finish_say(history, &last_say);
                }
                retried_engine = true;
                match complete(&prompt, "") {
                    Ok(raw) => strip_turn_markers(&raw),
                    Err(retry) => {
                        log::warn!("on-device reply failed after dropping the grammar: {retry}");
                        return finish_say(history, &last_say);
                    }
                }
            }
            Err(err) => return Err(err),
        };
        let decision = match parse_decision(&raw, &names) {
            Ok(decision) => decision,
            Err(_) if !looks_like_broken_json(&raw) => {
                let say = speakable(&raw, &last_say);
                history.push(ChatTurn { role: "assistant", text: say.clone() });
                return Ok(say);
            }
            Err(err) if !repaired => {
                repaired = true;
                history.push(ChatTurn { role: "assistant", text: raw });
                history.push(ChatTurn {
                    role: "user",
                    text: format!("That was not usable JSON ({err}). Reply with only the JSON object."),
                });
                continue;
            }
            Err(_) => {
                let say = speakable("", &last_say);
                history.push(ChatTurn { role: "assistant", text: say.clone() });
                return Ok(say);
            }
        };
        if let Some(name) = decision.tool.clone() {
            let output = call_tool(&name, &decision.args);
            history.push(ChatTurn { role: "assistant", text: raw });
            history.push(ChatTurn { role: "user", text: format!("Tool {name} result: {output}") });
            if !decision.say.trim().is_empty() {
                last_say = decision.say;
            } else if let Ok(value) = serde_json::from_str::<Value>(&output) {
                if let Some(detail) = value["detail"].as_str().or(value["error"].as_str()) {
                    last_say = detail.to_string();
                }
            }
            continue;
        }
        let say = if decision.say.trim().is_empty() { last_say } else { decision.say };
        let say = if say.trim().is_empty() {
            "I don't have a reply for that.".to_string()
        } else {
            say
        };
        history.push(ChatTurn { role: "assistant", text: say.clone() });
        return Ok(say);
    }
    if last_say.trim().is_empty() {
        Err("the on-device model did not finish the turn".into())
    } else {
        history.push(ChatTurn { role: "assistant", text: last_say.clone() });
        Ok(last_say)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_note_tool_and_a_plain_reply() {
        let names = tool_names();
        let note = parse_decision(
            r#"```json
            {"say":"Making that note.","tool":"create_note","args":{"text":"buy milk","title":"milk"}}
            ```"#,
            &names,
        )
        .unwrap();
        assert_eq!(note.tool.as_deref(), Some("create_note"));
        assert_eq!(note.args["text"], "buy milk");
        let plain = parse_decision(r#"{"say":"Hello.","tool":null,"args":{}}"#, &names).unwrap();
        assert!(plain.tool.is_none());
        assert_eq!(plain.say, "Hello.");
    }

    #[test]
    fn rejects_an_unknown_tool() {
        let names = tool_names();
        let err = parse_decision(r#"{"say":"no","tool":"wire_money","args":{}}"#, &names).unwrap_err();
        assert!(err.contains("wire_money"));
    }

    #[test]
    fn a_turn_does_not_apply_a_grammar() {
        let mut saw = String::new();
        let spoken = run_turns(
            "be brief",
            &mut Vec::new(),
            "hello",
            |_prompt, grammar| {
                saw = grammar.to_string();
                Ok(r#"{"say":"Hello.","tool":null,"args":{}}"#.into())
            },
            |_name, _args| "{}".into(),
        )
        .unwrap();
        assert!(saw.is_empty());
        assert_eq!(spoken, "Hello.");
        let err = r#"Unexpected empty grammar stack after accepting piece: {" (4913)"#;
        assert!(is_grammar_engine_failure(err));
        assert!(is_grammar_engine_failure("grammar failed: the reply stopped"));
        assert!(is_grammar_engine_failure("grammar rejected"));
        assert!(!is_grammar_engine_failure("out of memory"));
        assert!(!is_grammar_engine_failure("could not tokenize the prompt"));
    }

    #[test]
    fn local_prompt_includes_memory_and_cloud_tools_do_not() {
        let prompt = system_prompt("be brief", "", "\nPinned facts:\n- f1: I live in Zurich\n", "en");
        assert!(prompt.contains("I live in Zurich"));
        assert!(prompt.contains("remember_fact"));
        assert!(prompt.contains("Moonshine"));
        let german = system_prompt("be brief", "", "", "de");
        assert!(german.contains("German"));
        assert!(!german.contains("I live in Zurich"));
        let cloud = crate::actions::tool_definitions().to_string();
        assert!(!cloud.contains("remember_fact"));
        assert!(!cloud.contains("forget_fact"));
        assert!(cloud.contains("share_item"));
    }

    #[test]
    fn a_note_turn_calls_the_tool_then_speaks() {
        let mut replies = vec![
            r#"{"say":"Making that note.","tool":"create_note","args":{"text":"buy milk","title":"milk"}}"#.to_string(),
            r#"{"say":"Saved a note about milk.","tool":null,"args":{}}"#.to_string(),
        ];
        let mut calls = Vec::new();
        let mut history = Vec::new();
        let spoken = run_turns(
            "be brief",
            &mut history,
            "make a note to buy milk",
            |_prompt, _grammar| Ok(replies.remove(0)),
            |name, args| {
                calls.push((name.to_string(), args.clone()));
                r#"{"ok":true,"detail":"Saved a note: milk"}"#.into()
            },
        )
        .unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "create_note");
        assert_eq!(calls[0].1["text"], "buy milk");
        assert_eq!(spoken, "Saved a note about milk.");
    }

    #[test]
    fn a_search_error_is_handed_back_for_the_spoken_reply() {
        let mut replies = vec![
            r#"{"say":"","tool":"web_search","args":{"query":"weather"}}"#.to_string(),
            r#"{"say":"Web search needs a network connection, and this phone is offline.","tool":null,"args":{}}"#
                .to_string(),
        ];
        let mut saw = String::new();
        let spoken = run_turns(
            "be brief",
            &mut Vec::new(),
            "what's the weather",
            |_prompt, grammar| {
                assert!(grammar.is_empty());
                Ok(replies.remove(0))
            },
            |name, _args| {
                assert_eq!(name, "web_search");
                saw = crate::local::web::offline_tool_error();
                saw.clone()
            },
        )
        .unwrap();
        assert!(saw.contains("offline"));
        assert!(spoken.contains("offline"));
    }

    #[test]
    fn a_grammar_engine_failure_is_retried_and_still_runs_the_tool() {
        let mut calls = 0;
        let mut replies = vec![
            r#"{"say":"Making that note.","tool":"create_note","args":{"text":"buy milk","title":"milk"}}"#.to_string(),
            r#"{"say":"Saved a note about milk.","tool":null,"args":{}}"#.to_string(),
        ];
        let mut tools = Vec::new();
        let spoken = run_turns(
            "be brief",
            &mut Vec::new(),
            "make a note to buy milk",
            |_prompt, grammar| {
                assert!(grammar.is_empty());
                calls += 1;
                if calls == 1 {
                    Err(r#"grammar failed: Unexpected empty grammar stack after accepting piece: {" (4913)"#.into())
                } else {
                    Ok(replies.remove(0))
                }
            },
            |name, args| {
                tools.push((name.to_string(), args.clone()));
                r#"{"ok":true}"#.into()
            },
        )
        .unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].0, "create_note");
        assert_eq!(spoken, "Saved a note about milk.");
        assert!(calls >= 3);
    }

    #[test]
    fn a_grammar_engine_failure_speaks_plain_text_when_the_retry_is_not_json() {
        let mut calls = 0;
        let spoken = run_turns(
            "be brief",
            &mut Vec::new(),
            "give me a tiny little time to sit",
            |_prompt, _grammar| {
                calls += 1;
                if calls == 1 {
                    Err("grammar failed: Unexpected empty grammar stack".into())
                } else {
                    Ok("Take your time.".into())
                }
            },
            |_name, _args| "{}".into(),
        )
        .unwrap();
        assert_eq!(spoken, "Take your time.");
        assert_eq!(calls, 2);
    }

    #[test]
    fn a_grammar_engine_failure_that_keeps_failing_speaks_a_short_line() {
        let mut calls = 0;
        let spoken = run_turns(
            "be brief",
            &mut Vec::new(),
            "hello",
            |_prompt, _grammar| {
                calls += 1;
                Err("grammar rejected".into())
            },
            |_name, _args| "{}".into(),
        )
        .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(spoken, "Please say that again.");
    }

    #[test]
    fn other_model_errors_are_not_retried() {
        let mut calls = 0;
        let err = run_turns(
            "be brief",
            &mut Vec::new(),
            "hello",
            |_prompt, _grammar| {
                calls += 1;
                Err("out of memory".into())
            },
            |_name, _args| "{}".into(),
        )
        .unwrap_err();
        assert_eq!(calls, 1);
        assert!(err.contains("out of memory"));
    }

    #[test]
    fn plain_speech_is_spoken_without_another_generation() {
        let mut calls = 0;
        let spoken = run_turns(
            "be brief",
            &mut Vec::new(),
            "hello",
            |_prompt, grammar| {
                calls += 1;
                assert!(grammar.is_empty());
                Ok("Hello. Take your time.".into())
            },
            |_name, _args| panic!("a spoken reply should not call a tool"),
        )
        .unwrap();
        assert_eq!(spoken, "Hello. Take your time.");
        assert_eq!(calls, 1);
    }

    #[test]
    fn the_local_prompt_stays_short_enough_to_prefill() {
        let empty = system_prompt("", "", "", "en");
        let prompt = system_prompt("be brief", &"m".repeat(4000), &"l".repeat(4000), "en");
        assert!(empty.len() < 4000, "empty prompt is {} chars", empty.len());
        assert!(
            prompt.len() < empty.len() + 1400,
            "memory was not clipped: empty {} full {}",
            empty.len(),
            prompt.len()
        );
        assert!(prompt.contains("create_note"));
        assert!(prompt.contains("remember_fact"));
        assert!(prompt.contains("confirmed true"));
        assert!(prompt.contains("Moonshine Base"));
        assert!(!prompt.contains("Never send or place the call yourself"));
        let chat = super::prompt(&prompt, &[ChatTurn { role: "user", text: "hello".into() }]);
        assert!(chat.contains("<|im_start|>system\n"));
        assert!(chat.ends_with("<|im_start|>assistant\n"));
        let gemma = super::prompt_for("gemma", "be brief", &[ChatTurn { role: "user", text: "hello".into() }]);
        assert!(gemma.starts_with("<bos><start_of_turn>user\nbe brief\n\nhello<end_of_turn>\n"));
        assert!(gemma.ends_with("<start_of_turn>model\n"));
        let llama = super::prompt_for("llama3", "be brief", &[ChatTurn { role: "user", text: "hello".into() }]);
        assert!(llama.starts_with("<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\nbe brief<|eot_id|>"));
        assert!(llama.contains("<|start_header_id|>user<|end_header_id|>\n\nhello<|eot_id|>"));
        assert!(llama.ends_with("<|start_header_id|>assistant<|end_header_id|>\n\n"));
        let long = format!(
            "Can you create these recurring monthly events? {} On the 29th, the ZKB credit bill from 7:00 to 19:00.",
            "detail ".repeat(80)
        );
        assert!(long.chars().count() > HISTORY_TURN_CHARS);
        for family in ["chatml", "gemma", "llama3"] {
            let prompt = super::prompt_for(
                family,
                "be brief",
                &[
                    ChatTurn { role: "user", text: "a".repeat(800) },
                    ChatTurn { role: "assistant", text: "ok".into() },
                    ChatTurn { role: "user", text: long.clone() },
                ],
            );
            assert!(prompt.contains("On the 29th, the ZKB credit bill"), "{family}");
            assert!(!prompt.contains(&"a".repeat(HISTORY_TURN_CHARS + 1)), "{family}");
        }
        let marked = super::strip_turn_markers("{\"say\":\"Hi.\",\"tool\":null,\"args\":{}}<end_of_turn><|eot_id|>");
        assert_eq!(marked, "{\"say\":\"Hi.\",\"tool\":null,\"args\":{}}");
        let mut history = vec![
            ChatTurn { role: "user", text: "a".repeat(2000) },
            ChatTurn { role: "assistant", text: "b".repeat(2000) },
            ChatTurn { role: "user", text: "c".repeat(2000) },
        ];
        trim_history(&mut history);
        let total: usize = history.iter().map(|turn| turn.text.len()).sum();
        assert!(history.len() <= 2 || total <= super::HISTORY_CHAR_BUDGET);
    }
}
