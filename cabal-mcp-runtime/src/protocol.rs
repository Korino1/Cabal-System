use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use std::io::{BufRead, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageFormat {
    Ndjson,
    Framed,
}

#[derive(Debug, Clone)]
pub struct DecodedMessage {
    pub value: Value,
    pub format: MessageFormat,
}

pub fn read_jsonrpc_message<R: BufRead>(reader: &mut R) -> Result<Option<DecodedMessage>> {
    let mut first_line = String::new();
    loop {
        first_line.clear();
        let bytes = reader.read_line(&mut first_line)?;
        if bytes == 0 {
            return Ok(None);
        }
        if !first_line.trim().is_empty() {
            break;
        }
    }

    let trimmed = first_line.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        let value: Value = serde_json::from_str(first_line.trim_end())
            .context("failed to parse newline-delimited jsonrpc message")?;
        return Ok(Some(DecodedMessage {
            value,
            format: MessageFormat::Ndjson,
        }));
    }

    let mut content_length: Option<usize> = None;
    if first_line
        .to_ascii_lowercase()
        .starts_with("content-length:")
    {
        content_length = Some(parse_content_length(&first_line)?);
    }

    loop {
        // Some clients skip the required empty line after Content-Length and
        // start the JSON body immediately. Accept this variant to avoid
        // handshake timeouts in strict clients.
        if content_length.is_some() {
            let peek = reader.fill_buf()?;
            if !peek.is_empty() && (peek[0] == b'{' || peek[0] == b'[') {
                break;
            }
        }

        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if trimmed.to_ascii_lowercase().starts_with("content-length:") {
            content_length = Some(parse_content_length(trimmed)?);
        }
    }

    let len = content_length.ok_or_else(|| anyhow!("missing Content-Length header"))?;
    let mut body = vec![0u8; len];
    reader
        .read_exact(&mut body)
        .context("failed to read jsonrpc body")?;
    let value: Value = serde_json::from_slice(&body).context("invalid jsonrpc body")?;
    Ok(Some(DecodedMessage {
        value,
        format: MessageFormat::Framed,
    }))
}

pub fn write_jsonrpc_message<W: Write>(writer: &mut W, value: &Value) -> Result<()> {
    let payload = serde_json::to_vec(value)?;
    write!(writer, "Content-Length: {}\r\n\r\n", payload.len())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

pub fn write_jsonrpc_message_ndjson<W: Write>(writer: &mut W, value: &Value) -> Result<()> {
    let payload = serde_json::to_vec(value)?;
    writer.write_all(&payload)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn parse_content_length(line: &str) -> Result<usize> {
    let (_, rhs) = line
        .split_once(':')
        .ok_or_else(|| anyhow!("invalid content-length header"))?;
    let len: usize = rhs.trim().parse().context("invalid content-length value")?;
    if len == 0 {
        bail!("content-length must be > 0");
    }
    Ok(len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn read_jsonrpc_message_fails_on_invalid_ndjson() {
        let input = b"{invalid-json}\n";
        let mut reader = Cursor::new(input.as_slice());
        let err = read_jsonrpc_message(&mut reader).expect_err("must fail");
        assert!(
            err.to_string()
                .contains("failed to parse newline-delimited jsonrpc message")
        );
    }

    #[test]
    fn read_jsonrpc_message_fails_on_missing_content_length() {
        let input = b"Foo: bar\r\n\r\n";
        let mut reader = Cursor::new(input.as_slice());
        let err = read_jsonrpc_message(&mut reader).expect_err("must fail");
        assert!(err.to_string().contains("missing Content-Length header"));
    }

    #[test]
    fn read_jsonrpc_message_parses_ndjson_batch() {
        let input = b"[{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}]\n";
        let mut reader = Cursor::new(input.as_slice());
        let parsed = read_jsonrpc_message(&mut reader)
            .expect("read")
            .expect("some");
        assert_eq!(parsed.format, MessageFormat::Ndjson);
        let parsed = parsed.value;
        assert!(parsed.is_array());
        assert_eq!(parsed[0]["method"].as_str(), Some("ping"));
    }

    #[test]
    fn read_jsonrpc_message_parses_content_length_without_blank_separator() {
        let body = br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let mut input = format!("Content-Length: {}\r\n", body.len()).into_bytes();
        input.extend_from_slice(body);
        let mut reader = Cursor::new(input);
        let parsed = read_jsonrpc_message(&mut reader)
            .expect("read")
            .expect("some");
        assert_eq!(parsed.format, MessageFormat::Framed);
        let parsed = parsed.value;
        assert_eq!(parsed["method"].as_str(), Some("ping"));
        assert_eq!(parsed["id"].as_i64(), Some(1));
    }

    #[test]
    fn parse_content_length_rejects_invalid_value() {
        let err = parse_content_length("Content-Length: abc").expect_err("must fail");
        assert!(err.to_string().contains("invalid content-length value"));
    }

    #[test]
    fn parse_content_length_rejects_zero() {
        let err = parse_content_length("Content-Length: 0").expect_err("must fail");
        assert!(err.to_string().contains("content-length must be > 0"));
    }
}
