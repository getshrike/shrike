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
    let line = r#"{"protocolVersion":"1.0","id":"r","command":"version","payload":{},"extra":1}"#;
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
