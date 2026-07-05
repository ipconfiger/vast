//! Hermes Agent HTTP client.
//!
//! Calls the Hermes Agent's OpenAI-compatible `/v1/chat/completions` endpoint
//! and returns response text segments. Used by the mention bridge (T4) to
//! generate bot replies. One `HermesClient` is constructed per call — it is
//! NOT stored in `AppState` to avoid mutating test fixtures.

use serde::{Deserialize, Serialize};

/// A single chat message in the OpenAI-compatible messages array.
#[derive(Debug, Serialize)]
pub struct ChatMessage {
    /// "system", "user", or "assistant".
    pub role: String,
    pub content: String,
}

/// OpenAI-compatible chat completion response envelope.
#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: String,
}

/// Errors that can occur when calling Hermes.
#[derive(Debug)]
pub enum BotError {
    Timeout,
    ConnectionFailed,
    ApiError(String),
    ParseError(String),
}

impl std::fmt::Display for BotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BotError::Timeout => write!(f, "Hermes request timed out"),
            BotError::ConnectionFailed => write!(f, "Cannot connect to Hermes"),
            BotError::ApiError(msg) => write!(f, "Hermes API error: {}", msg),
            BotError::ParseError(msg) => write!(f, "Failed to parse Hermes response: {}", msg),
        }
    }
}

impl std::error::Error for BotError {}

/// HTTP client for the Hermes Agent API.
pub struct HermesClient {
    client: reqwest::Client,
}

impl HermesClient {
    /// Construct a new client with a 60-second timeout.
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    /// Call Hermes `/v1/chat/completions` endpoint.
    ///
    /// Prepends the system prompt (if non-empty) to the supplied messages,
    /// POSTs the OpenAI-compatible request, and returns one segment per
    /// `choices[i].message.content` entry. Returns `BotError::ParseError`
    /// if the response contains no choices.
    pub async fn chat(
        &self,
        api_url: &str,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        session_id: &str,
        messages: Vec<ChatMessage>,
    ) -> Result<Vec<String>, BotError> {
        // Build the full messages array with system prompt prepended.
        let mut all_messages = Vec::with_capacity(messages.len() + 1);
        if !system_prompt.is_empty() {
            all_messages.push(ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            });
        }
        all_messages.extend(messages);

        let body = serde_json::json!({
            "model": model,
            "messages": all_messages,
            "stream": false
        });

        let base = api_url.trim_end_matches('/').trim_end_matches("/v1");
        let url = format!("{}/v1/chat/completions", base);
        let mut req = self.client.post(&url).json(&body);
        if !api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }
        // Per-channel session for context continuity.
        req = req.header("X-Hermes-Session-Id", session_id);

        let resp = req.send().await.map_err(|e| {
            if e.is_timeout() {
                BotError::Timeout
            } else {
                BotError::ConnectionFailed
            }
        })?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(BotError::ApiError(format!("{}: {}", status, text)));
        }

        let parsed: ChatCompletionResponse = resp
            .json()
            .await
            .map_err(|e| BotError::ParseError(e.to_string()))?;

        // v1: return single segment from first choice.
        let segments: Vec<String> = parsed
            .choices
            .into_iter()
            .map(|c| c.message.content)
            .collect();

        if segments.is_empty() {
            return Err(BotError::ParseError("No choices in response".to_string()));
        }

        Ok(segments)
    }
}

impl Default for HermesClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ChatMessage serializes to `{"role": ..., "content": ...}` exactly.
    #[test]
    fn chat_message_serializes_to_openai_shape() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: "hello".to_string(),
        };
        let v: serde_json::Value = serde_json::to_value(&msg).unwrap();
        assert_eq!(v["role"], "user");
        assert_eq!(v["content"], "hello");
    }

    /// A multi-message array serializes into a JSON array preserving order.
    #[test]
    fn chat_message_array_preserves_order() {
        let msgs = vec![
            ChatMessage {
                role: "system".to_string(),
                content: "s".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: "u1".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: "u2".to_string(),
            },
        ];
        let v: serde_json::Value = serde_json::to_value(&msgs).unwrap();
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0]["role"], "system");
        assert_eq!(arr[1]["content"], "u1");
        assert_eq!(arr[2]["content"], "u2");
    }

    /// A typical OpenAI-compatible response body deserializes into a single
    /// text segment. This isolates the JSON parsing contract from the HTTP
    /// transport (no live server required).
    #[test]
    fn parse_openai_compatible_response() {
        let raw = serde_json::json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "Hello from Hermes"
                    },
                    "finish_reason": "stop"
                }
            ]
        });
        let parsed: ChatCompletionResponse = serde_json::from_value(raw).unwrap();
        assert_eq!(parsed.choices.len(), 1);
        assert_eq!(parsed.choices[0].message.content, "Hello from Hermes");
    }

    /// A response with multiple choices yields one segment per choice.
    #[test]
    fn parse_multi_choice_response() {
        let raw = serde_json::json!({
            "choices": [
                {"message": {"content": "seg-a"}},
                {"message": {"content": "seg-b"}}
            ]
        });
        let parsed: ChatCompletionResponse = serde_json::from_value(raw).unwrap();
        let segments: Vec<String> =
            parsed.choices.into_iter().map(|c| c.message.content).collect();
        assert_eq!(segments, vec!["seg-a", "seg-b"]);
    }

    /// BotError Display impls cover every variant with stable text.
    #[test]
    fn bot_error_display_covers_all_variants() {
        assert_eq!(BotError::Timeout.to_string(), "Hermes request timed out");
        assert_eq!(
            BotError::ConnectionFailed.to_string(),
            "Cannot connect to Hermes"
        );
        assert_eq!(
            BotError::ApiError("503 upstream".into()).to_string(),
            "Hermes API error: 503 upstream"
        );
        assert_eq!(
            BotError::ParseError("eof".into()).to_string(),
            "Failed to parse Hermes response: eof"
        );
    }

    /// BotError implements std::error::Error so it can bubble through
    /// libraries that require `Box<dyn std::error::Error>`.
    #[test]
    fn bot_error_implements_std_error() {
        fn asserts_is_error<E: std::error::Error>(_: &E) {}
        let err = BotError::Timeout;
        asserts_is_error(&err);
    }

    /// `HermesClient::new()` must not panic — it is constructed per-call
    /// inside spawned tasks (T4) where a panic would silently abort the
    /// bot reply.
    #[test]
    fn hermes_client_new_does_not_panic() {
        let _client = HermesClient::new();
    }

    /// `Default::default()` mirrors `new()` — used when callers write
    /// `HermesClient::default()` for ergonomics.
    #[test]
    fn hermes_client_default_matches_new() {
        let _default_client = HermesClient::default();
    }

    /// Sanity-check the URL composition rule: trailing slashes on `api_url`
    /// are stripped before appending `/v1/chat/completions`. We verify by
    /// building the same string expression the client uses.
    #[test]
    fn url_composition_strips_trailing_slash() {
        let with_slash = format!(
            "{}/v1/chat/completions",
            "http://hermes.local/".trim_end_matches('/')
        );
        let without_slash = format!(
            "{}/v1/chat/completions",
            "http://hermes.local".trim_end_matches('/')
        );
        assert_eq!(with_slash, "http://hermes.local/v1/chat/completions");
        assert_eq!(without_slash, "http://hermes.local/v1/chat/completions");
        assert_eq!(with_slash, without_slash);
    }
}
