use serde::{Deserialize, Serialize};
use serde_json::Value;
use shrike_core::PROTOCOL_VERSION;

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
