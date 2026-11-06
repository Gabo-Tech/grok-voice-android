//! Grammar-constrained tool turns. The model must emit one JSON object.
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
    actions::tool_definitions()
        .as_array()
        .map(|list| list.iter().filter_map(|tool| tool["name"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

pub fn system_prompt(custom: &str, memory: &str) -> String {
    let custom: String = custom.trim().chars().take(500).collect();
    let mut tools = String::new();
    if let Some(list) = actions::tool_definitions().as_array() {
        for tool in list {
            let name = tool["name"].as_str().unwrap_or("");
            let desc: String = tool["description"].as_str().unwrap_or("").chars().take(180).collect();
            tools.push_str(&format!("- {name}: {desc}\n"));
        }
    }
    format!(
        "{custom}\n\nYou are running on this phone. Reply with one JSON object and nothing else, shaped as {{\"say\":\"short spoken reply\",\"tool\":null,\"args\":{{}}}}. say is what you speak. tool is null or one name from the list. args is that tool's arguments. Call the tool before saying a note, reminder, alarm, timer, or calendar change is done. For open_browser, open_youtube, open_whatsapp, open_signal, open_email, open_call, and open_sms, say what will open and pass confirmed false until the user clearly agrees, then call the same tool with confirmed true. Nothing is sent, and a call is not placed, until they confirm. Times are local YYYY-MM-DDTHH:MM. If web_search returns an error, speak that error.\n\nTools:\n{tools}\n{memory}"
    )
}

pub fn grammar(names: &[String]) -> String {
    let mut alts = vec!["\"null\"".to_string()];
    for name in names {
        alts.push(format!("\"\\\"{name}\\\"\""));
    }
    format!(
        r#"root ::= "{{" ws "\"say\"" ws ":" ws string "," ws "\"tool\"" ws ":" ws tool "," ws "\"args\"" ws ":" ws object "}}" ws
tool ::= {tools}
object ::= "{{" ws "}}" | "{{" ws members ws "}}"
members ::= pair | pair "," ws members
pair ::= string ws ":" ws value
array ::= "[" ws "]" | "[" ws elements ws "]"
elements ::= value | value "," ws elements
value ::= string | number | object | array | "true" | "false" | "null"
string ::= "\"" chars "\""
chars ::= "" | char chars
char ::= [^"\\] | "\\" escape
escape ::= ["\\/bfnrt] | "u" hex hex hex hex
number ::= "-"? ("0" | [1-9] [0-9]*) ("." [0-9]+)? ([eE] [-+]? [0-9]+)?
ws ::= [ \t\n]*
hex ::= [0-9a-fA-F]
"#,
        tools = alts.join(" | "),
    )
}

pub fn prompt(system: &str, history: &[ChatTurn]) -> String {
    let mut out = String::new();
    out.push_str("<|im_start|>system\n");
    out.push_str(system.trim());
    out.push_str("<|im_end|>\n");
    for turn in history {
        let role = if turn.role == "assistant" { "assistant" } else { "user" };
        out.push_str("<|im_start|>");
        out.push_str(role);
        out.push('\n');
        out.push_str(turn.text.trim());
        out.push_str("<|im_end|>\n");
    }
    out.push_str("<|im_start|>assistant\n");
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
    while history.len() > 2 && total > 4000 {
        let removed = history.remove(0);
        total = total.saturating_sub(removed.text.len());
    }
}

/// One user turn. `complete` sees the Qwen chat prompt. `call_tool` runs the phone action.
pub fn run_turns(
    system: &str,
    history: &mut Vec<ChatTurn>,
    user_text: &str,
    mut complete: impl FnMut(&str, &str) -> Result<String, String>,
    mut call_tool: impl FnMut(&str, &Value) -> String,
) -> Result<String, String> {
    let names = tool_names();
    let grammar = grammar(&names);
    history.push(ChatTurn { role: "user", text: user_text.trim().to_string() });
    let mut repaired = false;
    let mut last_say = String::new();
    for _ in 0..4 {
        trim_history(history);
        let prompt = prompt(system, history);
        let raw = complete(&prompt, &grammar)?;
        let decision = match parse_decision(&raw, &names) {
            Ok(decision) => decision,
            Err(err) if !repaired => {
                repaired = true;
                history.push(ChatTurn { role: "assistant", text: raw });
                history.push(ChatTurn {
                    role: "user",
                    text: format!("That was not usable JSON ({err}). Reply with only the JSON object."),
                });
                continue;
            }
            Err(err) => return Err(err),
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
    fn grammar_lists_the_phone_tools() {
        let names = tool_names();
        let grammar = grammar(&names);
        assert!(grammar.contains("root ::="));
        assert!(grammar.contains(r#""\"create_note\"""#));
        assert!(grammar.contains(r#""\"set_reminder\"""#));
        assert!(grammar.contains(r#""\"web_search\"""#));
        assert!(grammar.contains(r#""\"open_call\"""#));
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
                assert!(grammar.contains("web_search"));
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
}
