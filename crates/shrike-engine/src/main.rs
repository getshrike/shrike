#![forbid(unsafe_code)]
//! JSONL engine entry point. The TypeScript host is the only intended client.
use shrike_protocol::{ProtocolError, Response, decode_request, encode_line};
use std::io::{BufRead, Write};

fn main() {
    let timing = std::env::var_os("SHRIKE_TIMING").is_some();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let read_start = std::time::Instant::now();
        let Ok(line) = line else { break };
        if timing {
            eprintln!(
                "shrike protocol: read_line={}ms",
                read_start.elapsed().as_millis()
            );
        }
        if line.trim().is_empty() {
            continue;
        }
        let decode_start = std::time::Instant::now();
        let decoded = decode_request(&line);
        if timing {
            eprintln!(
                "shrike protocol: decode={}ms",
                decode_start.elapsed().as_millis()
            );
        }
        let dispatch_start = std::time::Instant::now();
        let dispatched = match decoded {
            Ok(request) => match shrike_engine::dispatch(&request) {
                Ok(result) => Ok((request.id, result)),
                Err(error) => Err(encode_line(&Response::failure(
                    Some(request.id),
                    ProtocolError::new(error.code(), error.to_string()),
                ))),
            },
            Err(error) => Err(encode_line(&Response::failure(None, error))),
        };
        if timing {
            eprintln!(
                "shrike protocol: dispatch={}ms",
                dispatch_start.elapsed().as_millis()
            );
        }
        let write_start = std::time::Instant::now();
        let written = match &dispatched {
            Ok((id, result)) => shrike_engine::write_response(&mut stdout, id, result),
            Err(line) => stdout.write_all(line.as_bytes()),
        };
        if written.is_err() {
            break;
        }
        let _ = stdout.flush();
        if timing {
            eprintln!(
                "shrike protocol: write={}ms",
                write_start.elapsed().as_millis()
            );
        }
    }
}
