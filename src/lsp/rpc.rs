// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! JSON-RPC 2.0 over a byte stream with `Content-Length` framing, as the
//! Language Server Protocol uses it.

use std::io::{self, BufRead, Read, Write};

use serde_json::Value;

/// The largest message body read. A header that claims more is refused
/// instead of allocated.
pub const MAX_BODY: usize = 64 * 1024 * 1024;

/// What reading one frame found.
#[derive(Debug)]
pub enum Frame {
    /// A body that parsed as JSON.
    Message(Value),
    /// A body that was not JSON: answered with a parse error, then carry on.
    Garbled,
    /// The stream ended between frames.
    End,
}

/// Read one framed message. `Err` is a stream that cannot be re-synchronized
/// (a bad or oversized header): the caller stops.
pub fn read_frame(input: &mut impl BufRead) -> io::Result<Frame> {
    let mut length: Option<usize> = None;
    let mut saw_header = false;
    loop {
        let mut line = String::new();
        // Header lines are short; a runaway line is not a header.
        let n = (&mut *input).take(8192).read_line(&mut line)?;
        if n == 0 {
            return if saw_header {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "the stream ended inside a header",
                ))
            } else {
                Ok(Frame::End)
            };
        }
        if !line.ends_with('\n') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "a header line has no end",
            ));
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        saw_header = true;
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                length = Some(value.trim().parse().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "Content-Length is not a number")
                })?);
            }
        }
    }
    let Some(len) = length else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "a message has no Content-Length",
        ));
    };
    if len > MAX_BODY {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "a message is larger than the server reads",
        ));
    }
    let mut body = vec![0u8; len];
    input.read_exact(&mut body)?;
    Ok(match serde_json::from_slice(&body) {
        Ok(v) => Frame::Message(v),
        Err(_) => Frame::Garbled,
    })
}

/// Write one message with its header, and flush.
pub fn write_message(out: &mut impl Write, msg: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(msg).map_err(io::Error::other)?;
    write!(out, "Content-Length: {}\r\n\r\n", body.len())?;
    out.write_all(&body)?;
    out.flush()
}

/// A successful response.
pub fn response(id: &Value, result: Value) -> Value {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// An error response.
pub fn error(id: &Value, code: i64, message: &str) -> Value {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// A notification to the client.
pub fn notification(method: &str, params: Value) -> Value {
    serde_json::json!({"jsonrpc": "2.0", "method": method, "params": params})
}

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const NOT_INITIALIZED: i64 = -32002;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        let mut buf = Vec::new();
        write_message(&mut buf, &serde_json::json!({"a": 1})).unwrap();
        write_message(&mut buf, &serde_json::json!({"b": "é"})).unwrap();
        let mut r = io::Cursor::new(buf);
        assert!(matches!(read_frame(&mut r).unwrap(), Frame::Message(v) if v["a"] == 1));
        assert!(matches!(read_frame(&mut r).unwrap(), Frame::Message(v) if v["b"] == "é"));
        assert!(matches!(read_frame(&mut r).unwrap(), Frame::End));
    }

    #[test]
    fn bad_headers_stop_and_bad_bodies_do_not() {
        let mut r = io::Cursor::new(b"Content-Length: 99999999999\r\n\r\n".to_vec());
        assert!(read_frame(&mut r).is_err());
        let mut r = io::Cursor::new(b"X: y\r\n\r\n{}".to_vec());
        assert!(read_frame(&mut r).is_err());
        let mut r = io::Cursor::new(b"Content-Length: 3\r\n\r\nabc".to_vec());
        assert!(matches!(read_frame(&mut r).unwrap(), Frame::Garbled));
    }
}
