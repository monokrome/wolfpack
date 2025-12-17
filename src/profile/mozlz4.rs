use anyhow::{Context, Result, bail};

const MOZLZ4_MAGIC: &[u8] = b"mozLz40\0";

pub fn decode_mozlz4(data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < MOZLZ4_MAGIC.len() + 4 {
        bail!("Data too short to be valid mozLz4");
    }

    if &data[..MOZLZ4_MAGIC.len()] != MOZLZ4_MAGIC {
        bail!("Invalid mozLz4 magic header");
    }

    let size_bytes = &data[MOZLZ4_MAGIC.len()..MOZLZ4_MAGIC.len() + 4];
    let decompressed_size =
        u32::from_le_bytes([size_bytes[0], size_bytes[1], size_bytes[2], size_bytes[3]]) as usize;

    let compressed = &data[MOZLZ4_MAGIC.len() + 4..];

    let decompressed = lz4_flex::decompress(compressed, decompressed_size)
        .context("Failed to decompress lz4 data")?;

    Ok(decompressed)
}

pub fn encode_mozlz4(data: &[u8]) -> Vec<u8> {
    let compressed = lz4_flex::compress(data);

    let mut result = Vec::with_capacity(MOZLZ4_MAGIC.len() + 4 + compressed.len());
    result.extend_from_slice(MOZLZ4_MAGIC);
    result.extend_from_slice(&(data.len() as u32).to_le_bytes());
    result.extend_from_slice(&compressed);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mozlz4_roundtrip() {
        let original = b"Hello, wolfpack! This is a test of the mozLz4 compression.";
        let compressed = encode_mozlz4(original);
        let decompressed = decode_mozlz4(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }
}
