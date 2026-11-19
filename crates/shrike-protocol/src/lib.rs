#![forbid(unsafe_code)]
//! The versioned, line-delimited JSON contract between the TypeScript host and
//! the Rust engine.
//!
//! One request per line, one response per line. A response is either a complete
//! success (`ok: true`, non-null `result`, null `error`) or a complete failure
//! (`ok: false`, null `result`, non-null `error`). A partial response is a
//! protocol error and fails closed.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use shrike_core::PROTOCOL_VERSION;

/// Commands implemented by the engine.
pub const COMMANDS: &[&str] = &[
    "ping",
    "version",
    "analyze",
    "scan",
    "delta",
    "plan",
    "proof",
    "join-build",
    "augment-build",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub protocol_version: String,
    pub id: String,
    pub command: String,
    #[serde(default)]
    pub payload: Value,
}

impl Request {
    #[must_use]
    pub fn new(id: impl Into<String>, command: impl Into<String>, payload: Value) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id: id.into(),
            command: command.into(),
            payload,
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ProtocolError::new(
                "unsupported_protocol",
                format!(
                    "protocolVersion {} is not supported; expected {}",
                    self.protocol_version, PROTOCOL_VERSION
                ),
            ));
        }
        if self.id.trim().is_empty() {
            return Err(ProtocolError::new("missing_id", "request id is required"));
        }
        if self.command.trim().is_empty() {
            return Err(ProtocolError::new(
                "missing_command",
                "request command is required",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Response {
    pub protocol_version: String,
    pub id: Option<String>,
    pub ok: bool,
    pub result: Option<Value>,
    pub error: Option<ProtocolError>,
}

impl Response {
    #[must_use]
    pub fn success(id: impl Into<String>, result: Value) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id: Some(id.into()),
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    #[must_use]
    pub fn failure(id: Option<String>, error: ProtocolError) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id,
            ok: false,
            result: None,
            error: Some(error),
        }
    }

    /// A partial result, a missing result, or a missing error is invalid.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ProtocolError::new(
                "unsupported_protocol",
                "response protocol version does not match",
            ));
        }
        match (self.ok, self.result.as_ref(), self.error.as_ref()) {
            (true, Some(_), None) => Ok(()),
            (false, None, Some(_)) => Ok(()),
            (true, None, _) => Err(ProtocolError::new(
                "partial_result",
                "ok response is missing result",
            )),
            (true, _, Some(_)) => Err(ProtocolError::new(
                "partial_result",
                "ok response must not carry an error",
            )),
            (false, Some(_), _) => Err(ProtocolError::new(
                "partial_result",
                "error response must not carry a result",
            )),
            (false, _, None) => Err(ProtocolError::new(
                "partial_result",
                "error response is missing error",
            )),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl ProtocolError {
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    #[must_use]
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

/// Encode one response as a single line, including the trailing newline.
///
/// # Panics
/// Panics only if a response contains a non-serializable value; all protocol
/// values are JSON values.
#[must_use]
pub fn encode_line(response: &Response) -> String {
    let mut line = serde_json::to_string(response).expect("protocol response is serializable");
    line.push('\n');
    line
}

/// Decode one request line. Blank lines are ignored by the caller.
pub fn decode_request(line: &str) -> Result<Request, ProtocolError> {
    let request: Request = serde_json::from_str(line).map_err(|error| {
        ProtocolError::new(
            "malformed_request",
            format!("invalid JSON request: {error}"),
        )
    })?;
    request.validate()?;
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn valid_request_round_trips() {
        let line = r#"{"protocolVersion":"1.0","id":"req_1","command":"version","payload":{}}"#;
        let request = decode_request(line).unwrap();
        assert_eq!(request.command, "version");
        assert_eq!(request.id, "req_1");
    }

    #[test]
    fn malformed_json_is_a_protocol_error() {
        let error = decode_request("{").unwrap_err();
        assert_eq!(error.code, "malformed_request");
    }

    #[test]
    fn unknown_top_level_field_is_rejected() {
        let line =
            r#"{"protocolVersion":"1.0","id":"r","command":"version","payload":{},"extra":1}"#;
        assert_eq!(decode_request(line).unwrap_err().code, "malformed_request");
    }

    #[test]
    fn missing_id_is_rejected() {
        let line = r#"{"protocolVersion":"1.0","id":"","command":"version","payload":{}}"#;
        assert_eq!(decode_request(line).unwrap_err().code, "missing_id");
    }

    #[test]
    fn wrong_protocol_is_rejected() {
        let line = r#"{"protocolVersion":"2.0","id":"r","command":"version","payload":{}}"#;
        assert_eq!(
            decode_request(line).unwrap_err().code,
            "unsupported_protocol"
        );
    }

    #[test]
    fn partial_results_fail_closed() {
        let mut response = Response::success("r", json!({"ok": true}));
        response.result = None;
        assert_eq!(response.validate().unwrap_err().code, "partial_result");

        let mut response = Response::failure(Some("r".into()), ProtocolError::new("x", "y"));
        response.result = Some(json!({}));
        assert_eq!(response.validate().unwrap_err().code, "partial_result");
    }

    #[test]
    fn encodes_one_line() {
        let line = encode_line(&Response::success("r", json!({"version":"1.0.0"})));
        assert!(line.ends_with('\n'));
        assert_eq!(line.matches('\n').count(), 1);
    }
}
