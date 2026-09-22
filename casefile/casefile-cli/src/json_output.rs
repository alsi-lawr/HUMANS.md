use std::io::{self, Write};

use anyhow::Result;
use serde::Serialize;

pub(super) const MAX_JSON_BYTES: usize = 8 * 1024 * 1024;
const LIMIT_MESSAGE: &str = "JSON response exceeds the 8 MiB limit; use snapshot, an exact scoped record_index, record_detail or diagnostics query";

struct BoundedBuffer(Vec<u8>);
impl Write for BoundedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_JSON_BYTES - self.0.len() {
            return Err(io::Error::other(LIMIT_MESSAGE));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn encode(value: &impl Serialize) -> Result<Vec<u8>> {
    encode_format(value, false)
}

fn encode_format(value: &impl Serialize, pretty: bool) -> Result<Vec<u8>> {
    let mut buffer = BoundedBuffer(Vec::new());
    if pretty {
        serde_json::to_writer_pretty(&mut buffer, value)?;
    } else {
        serde_json::to_writer(&mut buffer, value)?;
    }
    buffer.write_all(b"\n")?;
    Ok(buffer.0)
}

pub(super) fn write_json(output: &mut impl Write, value: &impl Serialize) -> Result<()> {
    let bytes = encode_format(value, true)?;
    output.write_all(&bytes)?;
    output.flush()?;
    Ok(())
}

pub(super) fn write_message(output: &mut impl Write, value: &impl Serialize) -> Result<()> {
    let bytes = encode(value)?;
    output.write_all(&bytes)?;
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overflow_leaves_output_empty_including_json_escaping_expansion() {
        let mut output = Vec::new();
        let value = "\0".repeat(MAX_JSON_BYTES / 5);
        assert!(write_json(&mut output, &value).is_err());
        assert!(output.is_empty());
        let boundary = "a".repeat(MAX_JSON_BYTES - 3);
        write_json(&mut output, &boundary).unwrap();
        assert_eq!(output.len(), MAX_JSON_BYTES);
        assert_eq!(serde_json::from_slice::<String>(&output).unwrap(), boundary);
    }
}
