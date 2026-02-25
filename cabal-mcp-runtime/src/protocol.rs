use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use std::io::{BufRead, Write};

pub fn read_jsonrpc_message<R: BufRead>(reader: &mut R) -> Result<Option<Value>> {
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

    if first_line.trim_start().starts_with('{') {
        let value: Value = serde_json::from_str(first_line.trim_end())
            .context("failed to parse newline-delimited jsonrpc message")?;
        return Ok(Some(value));
    }

    let mut content_length: Option<usize> = None;
    if first_line
        .to_ascii_lowercase()
        .starts_with("content-length:")
    {
        content_length = Some(parse_content_length(&first_line)?);
    }

    loop {
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
    Ok(Some(value))
}

pub fn write_jsonrpc_message<W: Write>(writer: &mut W, value: &Value) -> Result<()> {
    let payload = serde_json::to_vec(value)?;
    write!(writer, "Content-Length: {}\r\n\r\n", payload.len())?;
    writer.write_all(&payload)?;
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
