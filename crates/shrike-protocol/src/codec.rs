use serde::Serialize;
use shrike_core::PROTOCOL_VERSION;

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

/// Encode a success response without materializing the result as a
/// `serde_json::Value`. The envelope is byte-for-byte the same shape as
/// [`encode_line`] on an equivalent [`Response`].
///
/// # Panics
/// Panics only if a `Serialize` implementation is broken.
#[must_use]
pub fn encode_success_line<T: Serialize>(id: &str, result: &T) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Success<'a, T: Serialize> {
        protocol_version: &'static str,
        id: &'a str,
        ok: bool,
        result: &'a T,
        error: Option<&'a ProtocolError>,
    }

    let mut line = serde_json::to_string(&Success {
        protocol_version: PROTOCOL_VERSION,
        id,
        ok: true,
        result,
        error: None,
    })
    .expect("protocol response is serializable");
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
