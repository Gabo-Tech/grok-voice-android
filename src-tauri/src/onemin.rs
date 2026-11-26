//! 1min.ai OpenAI-compatible text. Native HTTP only. One attempt per request:
//! a retry would be a second charged generation.
//!
//! Docs: https://docs.1min.ai/docs/api/openai-compatible

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const BASE: &str = "https://api.1min.ai/openai/v1";
pub const DEFAULT_MODEL: &str = "gpt-4.1-mini";
/// These three accept JSON, and they do not accept function tools.
pub const NO_TOOL_MODELS: &[&str] = &["gpt-6.1-sol", "gpt-6.2-sol", "gpt-6.2-astra"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailClass {
    Quota,
    Rate,
    Auth,
    Timeout,
    Incomplete,
    Server,
    Other,
}

impl FailClass {
    pub fn fails_over(self) -> bool {
        !matches!(self, FailClass::Other)
    }
}

pub fn supports_tools(model: &str) -> bool {
    !NO_TOOL_MODELS.iter().any(|id| id.eq_ignore_ascii_case(model.trim()))
}

pub fn classify_failure(
    status: u16,
    error_type: &str,
    error_code: &str,
    timed_out: bool,
    incomplete: bool,
) -> FailClass {
    if timed_out {
        return FailClass::Timeout;
    }
    if incomplete {
        return FailClass::Incomplete;
    }
    let kind = format!("{error_type} {error_code}").to_ascii_lowercase();
    if status == 429 && kind.contains("insufficient_quota") {
        return FailClass::Quota;
    }
    if status == 429 || kind.contains("rate_limit") || kind.contains("too_many") {
        return FailClass::Rate;
    }
    if status == 401 || status == 403 || kind.contains("authentication") || kind.contains("invalid_api_key") {
        return FailClass::Auth;
    }
    if (500..600).contains(&status) {
        return FailClass::Server;
    }
    FailClass::Other
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolDelta {
    pub index: u32,
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamedChat {
    pub content: String,
    pub tool_calls: Vec<ToolDelta>,
    pub finish_reason: String,
    pub saw_done: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamError {
    Incomplete,
    BadFrame,
}

/// Parse `chat.completion.chunk` frames and require a final `[DONE]`.
pub fn parse_sse(body: &str) -> Result<StreamedChat, StreamError> {
    let mut content = String::new();
    let mut calls: Vec<ToolDelta> = Vec::new();
    let mut finish = String::new();
    let mut saw_done = false;
    let mut saw_chunk = false;
    for raw in body.split('\n') {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" {
            saw_done = true;
            break;
        }
        let value: Value = serde_json::from_str(data).map_err(|_| StreamError::BadFrame)?;
        if value["object"] != "chat.completion.chunk" && !value["choices"].is_array() {
            return Err(StreamError::BadFrame);
        }
        saw_chunk = true;
        let choice = &value["choices"][0];
        if let Some(reason) = choice["finish_reason"].as_str() {
            if !reason.is_empty() && reason != "null" {
                finish = reason.to_string();
            }
        }
        let delta = &choice["delta"];
        if let Some(text) = delta["content"].as_str() {
            content.push_str(text);
        }
        if let Some(list) = delta["tool_calls"].as_array() {
            for tool in list {
                let index = tool["index"].as_u64().unwrap_or(0) as u32;
                let slot = if let Some(found) = calls.iter_mut().find(|call| call.index == index) {
                    found
                } else {
                    calls.push(ToolDelta {
                        index,
                        id: String::new(),
                        name: String::new(),
                        arguments: String::new(),
                    });
                    calls.last_mut().expect("just pushed")
                };
                if let Some(id) = tool["id"].as_str() {
                    slot.id.push_str(id);
                }
                if let Some(name) = tool["function"]["name"].as_str() {
                    slot.name.push_str(name);
                }
                if let Some(args) = tool["function"]["arguments"].as_str() {
                    slot.arguments.push_str(args);
                }
            }
        }
    }
    if !saw_done {
        return Err(StreamError::Incomplete);
    }
    if !saw_chunk && content.is_empty() && calls.is_empty() {
        return Err(StreamError::Incomplete);
    }
    Ok(StreamedChat {
        content,
        tool_calls: calls,
        finish_reason: finish,
        saw_done,
    })
}

pub fn chat_tools(definitions: &Value) -> Value {
    let Some(list) = definitions.as_array() else {
        return json!([]);
    };
    let tools: Vec<Value> = list
        .iter()
        .filter_map(|tool| {
            let name = tool["name"].as_str()?;
            Some(json!({
                "type": "function",
                "function": {
                    "name": name,
                    "description": tool["description"].as_str().unwrap_or(""),
                    "parameters": tool.get("parameters").cloned().unwrap_or_else(|| json!({ "type": "object", "properties": {} }))
                }
            }))
        })
        .collect();
    json!(tools)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListedModel {
    pub id: String,
    pub owned_by: String,
    pub tools: bool,
}

pub fn models_from_body(body: &str) -> Result<Vec<ListedModel>, String> {
    let value: Value = serde_json::from_str(body).map_err(|_| "The model list was not JSON.".to_string())?;
    let Some(data) = value["data"].as_array() else {
        return Err("The model list had no data.".into());
    };
    let mut models = Vec::new();
    for row in data {
        let Some(id) = row["id"].as_str() else { continue };
        let id = id.trim();
        if id.is_empty() || id.len() > 80 {
            continue;
        }
        models.push(ListedModel {
            owned_by: row["owned_by"].as_str().unwrap_or("").to_string(),
            tools: supports_tools(id),
            id: id.to_string(),
        });
    }
    models.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(models)
}

#[derive(Clone, Debug)]
pub struct TurnFailure {
    pub class: FailClass,
    pub message: String,
}

impl TurnFailure {
    pub fn failover_text(&self) -> String {
        if self.class.fails_over() {
            format!("onemin-failover: {}", self.message)
        } else {
            format!("onemin-keep: {}", self.message)
        }
    }
}

fn error_fields(body: &str) -> (String, String, String) {
    let value: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let message = value["error"]["message"]
        .as_str()
        .unwrap_or("")
        .chars()
        .take(180)
        .collect();
    let kind = value["error"]["type"].as_str().unwrap_or("").to_string();
    let code = value["error"]["code"].as_str().unwrap_or("").to_string();
    (message, kind, code)
}

pub fn failure_from_http(status: u16, body: &str, timed_out: bool) -> TurnFailure {
    let (message, kind, code) = error_fields(body);
    let class = classify_failure(status, &kind, &code, timed_out, false);
    let message = if message.is_empty() {
        format!("1min.ai returned {status}")
    } else {
        message
    };
    TurnFailure { class, message }
}

pub fn failure_from_stream(err: StreamError) -> TurnFailure {
    let class = match err {
        StreamError::Incomplete => FailClass::Incomplete,
        StreamError::BadFrame => FailClass::Other,
    };
    TurnFailure {
        class,
        message: match err {
            StreamError::Incomplete => "The 1min.ai stream closed before it finished.".into(),
            StreamError::BadFrame => "The 1min.ai stream was not a chat completion.".into(),
        },
    }
}

/// One non-streaming-looking POST that still sets `stream: true`. No retry.
pub fn post_chat(
    client: &reqwest::blocking::Client,
    key: &str,
    body: &Value,
) -> Result<StreamedChat, TurnFailure> {
    let response = client
        .post(format!("{BASE}/chat/completions"))
        .bearer_auth(key)
        .header("accept", "text/event-stream")
        .json(body)
        .send();
    let response = match response {
        Ok(response) => response,
        Err(err) => {
            return Err(TurnFailure {
                class: FailClass::Timeout,
                message: if err.is_timeout() {
                    "1min.ai timed out.".into()
                } else {
                    "1min.ai could not be reached.".into()
                },
            });
        }
    };
    let status = response.status().as_u16();
    let text = response.text().unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(failure_from_http(status, &text, false));
    }
    parse_sse(&text).map_err(failure_from_stream)
}

pub fn post_models(client: &reqwest::blocking::Client, key: &str) -> Result<Vec<ListedModel>, TurnFailure> {
    let response = match client.get(format!("{BASE}/models")).bearer_auth(key).send() {
        Ok(response) => response,
        Err(err) => {
            return Err(TurnFailure {
                class: FailClass::Timeout,
                message: if err.is_timeout() {
                    "1min.ai timed out.".into()
                } else {
                    "1min.ai could not be reached.".into()
                },
            });
        }
    };
    let status = response.status().as_u16();
    let text = response.text().unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(failure_from_http(status, &text, false));
    }
    models_from_body(&text).map_err(|message| TurnFailure { class: FailClass::Other, message })
}

fn request_body(model: &str, messages: &Value, tools: &Value) -> Value {
    let mut body = json!({
        "model": model,
        "messages": messages,
        "stream": true,
        "max_completion_tokens": 512
    });
    if supports_tools(model) && tools.as_array().is_some_and(|list| !list.is_empty()) {
        body["tools"] = tools.clone();
        body["tool_choice"] = json!("auto");
        body["parallel_tool_calls"] = json!(false);
    }
    body
}

/// Run the tool loop. Each POST is a single attempt.
pub fn converse<F, T>(
    model: &str,
    system: &str,
    user: &str,
    tools: &Value,
    mut turn: F,
    mut run_tool: T,
) -> Result<String, TurnFailure>
where
    F: FnMut(&Value) -> Result<StreamedChat, TurnFailure>,
    T: FnMut(&str, &Value) -> String,
{
    let mut messages = vec![
        json!({ "role": "system", "content": system }),
        json!({ "role": "user", "content": user }),
    ];
    let chat_tools = chat_tools(tools);
    for _ in 0..4 {
        let streamed = turn(&request_body(model, &json!(messages), &chat_tools))?;
        if streamed.tool_calls.is_empty() {
            let said = streamed.content.trim().to_string();
            if said.is_empty() {
                return Err(TurnFailure {
                    class: FailClass::Other,
                    message: "1min.ai returned an empty reply.".into(),
                });
            }
            return Ok(said);
        }
        let calls: Vec<Value> = streamed
            .tool_calls
            .iter()
            .map(|call| {
                json!({
                    "id": if call.id.is_empty() { format!("call-{}", call.index) } else { call.id.clone() },
                    "type": "function",
                    "function": { "name": call.name, "arguments": call.arguments }
                })
            })
            .collect();
        messages.push(json!({
            "role": "assistant",
            "content": if streamed.content.is_empty() { Value::Null } else { json!(streamed.content) },
            "tool_calls": calls
        }));
        for call in &streamed.tool_calls {
            let args: Value = serde_json::from_str(&call.arguments).unwrap_or(Value::Null);
            let id = if call.id.is_empty() { format!("call-{}", call.index) } else { call.id.clone() };
            let output = run_tool(&call.name, &args);
            messages.push(json!({
                "role": "tool",
                "tool_call_id": id,
                "content": output
            }));
        }
    }
    Err(TurnFailure {
        class: FailClass::Other,
        message: "1min.ai kept calling tools without a spoken reply.".into(),
    })
}

pub fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(20))
        .redirect(crate::http::api_redirect())
        .build()
        .expect("1min client")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failover_classes_match_the_charged_failures() {
        assert_eq!(
            classify_failure(429, "insufficient_quota", "insufficient_quota", false, false),
            FailClass::Quota
        );
        assert_eq!(classify_failure(429, "rate_limit_error", "", false, false), FailClass::Rate);
        assert_eq!(
            classify_failure(401, "authentication_error", "invalid_api_key", false, false),
            FailClass::Auth
        );
        assert_eq!(classify_failure(0, "", "", true, false), FailClass::Timeout);
        assert_eq!(classify_failure(200, "", "", false, true), FailClass::Incomplete);
        assert_eq!(classify_failure(503, "server_error", "", false, false), FailClass::Server);
        assert_eq!(classify_failure(400, "invalid_request_error", "", false, false), FailClass::Other);
        assert!(FailClass::Quota.fails_over());
        assert!(FailClass::Incomplete.fails_over());
        assert!(!FailClass::Other.fails_over());
        assert!(!supports_tools("gpt-6.1-sol"));
        assert!(!supports_tools("GPT-6.2-Astra"));
        assert!(supports_tools("gpt-4.1-mini"));
    }

    #[test]
    fn sse_joins_chunks_and_tool_deltas_then_requires_done() {
        let body = "\
data: {\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hello\"},\"finish_reason\":null}]}\n\
data: {\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" there\"},\"finish_reason\":\"stop\"}]}\n\
data: [DONE]\n";
        let parsed = parse_sse(body).unwrap();
        assert_eq!(parsed.content, "Hello there");
        assert_eq!(parsed.finish_reason, "stop");
        assert!(parsed.saw_done);

        let tools = "\
data: {\"object\":\"chat.completion.chunk\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"get_settings\",\"arguments\":\"\"}}]}}]}\n\
data: {\"object\":\"chat.completion.chunk\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{}\"}}]}}]}\n\
data: [DONE]\n";
        let parsed = parse_sse(tools).unwrap();
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].name, "get_settings");
        assert_eq!(parsed.tool_calls[0].arguments, "{}");
        assert!(parse_sse("data: {\"object\":\"chat.completion.chunk\",\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n").is_err());
    }

    #[test]
    fn model_list_flags_the_three_models_without_tools() {
        let body = r#"{"object":"list","data":[{"id":"gpt-4.1-mini","owned_by":"openai"},{"id":"gpt-6.2-sol","owned_by":"openai"},{"id":"gpt-6.1-sol","owned_by":"openai"}]}"#;
        let models = models_from_body(body).unwrap();
        assert_eq!(models[0].id, "gpt-4.1-mini");
        assert!(models[0].tools);
        assert!(!models.iter().find(|model| model.id == "gpt-6.2-sol").unwrap().tools);
    }

    #[test]
    fn a_tool_round_is_one_request_and_a_failure_is_not_retried() {
        let mut calls = 0;
        let result = converse(
            "gpt-4.1-mini",
            "Be brief.",
            "What model?",
            &json!([]),
            |_| {
                calls += 1;
                Ok(StreamedChat {
                    content: "gpt-4.1-mini".into(),
                    tool_calls: Vec::new(),
                    finish_reason: "stop".into(),
                    saw_done: true,
                })
            },
            |_name, _args| String::new(),
        );
        assert_eq!(result.unwrap(), "gpt-4.1-mini");
        assert_eq!(calls, 1);

        let mut attempts = 0;
        let failed = converse(
            "gpt-4.1-mini",
            "Be brief.",
            "Hi",
            &json!([]),
            |_| {
                attempts += 1;
                Err(TurnFailure { class: FailClass::Quota, message: "insufficient_quota".into() })
            },
            |_name, _args| String::new(),
        );
        assert_eq!(attempts, 1);
        assert!(failed.unwrap_err().failover_text().contains("insufficient_quota"));
    }
}
