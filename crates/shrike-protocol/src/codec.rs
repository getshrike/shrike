use crate::types::{ProtocolError, Request, Response};

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
