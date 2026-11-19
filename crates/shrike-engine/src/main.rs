#![forbid(unsafe_code)]
//! JSONL engine entry point. The TypeScript host is the only intended client.
use shrike_protocol::{ProtocolError, Response, decode_request, encode_line};
use std::io::{BufRead, Write};

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let response = match decode_request(&line) {
            Ok(request) => match shrike_engine::dispatch(&request) {
                Ok(result) => Response::success(request.id, result),
                Err(error) => Response::failure(
                    Some(request.id),
                    ProtocolError::new(error.code(), error.to_string()),
                ),
            },
            Err(error) => Response::failure(None, error),
        };
        let encoded = encode_line(&response);
        if stdout.write_all(encoded.as_bytes()).is_err() {
            break;
        }
        let _ = stdout.flush();
    }
}
