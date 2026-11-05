//! Strict conflict text transport. Preserve the original encoding, one transport
//! BOM and every line ending; reject unsupported binary/legacy/LFS content.
use crate::operation::Error;
pub const MAX_BYTES: usize = 512 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16LittleEndianBom,
    Utf16BigEndianBom,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Content {
    pub text: String,
    pub encoding: Encoding,
}
fn supported(text: &str) -> bool {
    !text.contains('\0')
        && !text.starts_with("version https://git-lfs.github.com/spec/v1")
        && text.bytes().filter(|b| *b == b'\n').count() <= 20000
        && text
            .split('\n')
            .all(|line| line.encode_utf16().count() <= 16384)
}
pub fn decode(bytes: &[u8]) -> Option<Content> {
    if bytes.len() > MAX_BYTES
        || bytes.starts_with(&[0xff, 0xfe, 0, 0])
        || bytes.starts_with(&[0, 0, 0xfe, 0xff])
    {
        return None;
    }
    let (encoding, preamble) = if bytes.starts_with(&[0xff, 0xfe]) {
        (Encoding::Utf16LittleEndianBom, 2)
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        (Encoding::Utf16BigEndianBom, 2)
    } else if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        (Encoding::Utf8Bom, 3)
    } else {
        (Encoding::Utf8, 0)
    };
    let bytes = &bytes[preamble..];
    let text = match encoding {
        Encoding::Utf8 | Encoding::Utf8Bom => std::str::from_utf8(bytes).ok()?.to_owned(),
        _ => {
            let (pairs, remainder) = bytes.as_chunks::<2>();
            if !remainder.is_empty() {
                return None;
            }
            let units = pairs
                .iter()
                .map(|pair| {
                    if encoding == Encoding::Utf16LittleEndianBom {
                        u16::from_le_bytes(*pair)
                    } else {
                        u16::from_be_bytes(*pair)
                    }
                })
                .collect::<Vec<_>>();
            String::from_utf16(&units).ok()?
        }
    };
    supported(&text).then_some(Content { text, encoding })
}
pub fn encode(text: &str, encoding: Encoding) -> Result<Vec<u8>, Error> {
    if !supported(text) {
        return Err(Error::Code("ConflictTextLimit"));
    }
    let mut bytes = match encoding {
        Encoding::Utf8 => vec![],
        Encoding::Utf8Bom => vec![0xef, 0xbb, 0xbf],
        Encoding::Utf16LittleEndianBom => vec![0xff, 0xfe],
        Encoding::Utf16BigEndianBom => vec![0xfe, 0xff],
    };
    match encoding {
        Encoding::Utf8 | Encoding::Utf8Bom => {
            if text.len() > MAX_BYTES - bytes.len() {
                return Err(Error::Code("ConflictTextLimit"));
            }
            bytes.extend_from_slice(text.as_bytes());
        }
        _ => {
            for unit in text.encode_utf16() {
                if bytes.len() > MAX_BYTES - 2 {
                    return Err(Error::Code("ConflictTextLimit"));
                }
                bytes.extend(if encoding == Encoding::Utf16LittleEndianBom {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
        }
    }
    Ok(bytes)
}
pub fn has_markers(text: &str) -> bool {
    text.split('\n').any(|line| {
        ["<<<<<<<", "=======", ">>>>>>>"]
            .iter()
            .any(|marker| line.starts_with(marker))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_transport_bom_content_bom_unicode_and_mixed_line_endings() {
        let text = "\u{feff}שלום 😀\r\nsecond\nthird\rfinal";
        for encoding in [
            Encoding::Utf8,
            Encoding::Utf8Bom,
            Encoding::Utf16LittleEndianBom,
            Encoding::Utf16BigEndianBom,
        ] {
            let bytes = encode(text, encoding).unwrap();
            let decoded = decode(&bytes).unwrap();
            if encoding == Encoding::Utf8 {
                // An initial UTF-8 U+FEFF is necessarily observed as a transport
                // BOM. Saving its decoded encoding still reproduces exact bytes.
                assert_eq!(decoded.encoding, Encoding::Utf8Bom);
            } else {
                assert_eq!(decoded.text, text);
                assert_eq!(decoded.encoding, encoding);
            }
            assert_eq!(encode(&decoded.text, decoded.encoding).unwrap(), bytes);
        }
    }
    #[test]
    fn rejects_malformed_unicode_utf32_binary_lfs_and_utf16_column_overflow() {
        for bytes in [
            &[0xff][..],
            &[0xff, 0xfe, 0x00],
            &[0xff, 0xfe, 0x00, 0xd8],
            &[0xfe, 0xff, 0xdc, 0],
            &[0xff, 0xfe, 0, 0, b'a', 0, 0, 0],
            &[0, 0, 0xfe, 0xff],
            b"a\0b",
            b"version https://git-lfs.github.com/spec/v1\n",
        ] {
            assert!(decode(bytes).is_none());
        }
        assert!(encode(&"😀".repeat(8193), Encoding::Utf8).is_err());
        assert!(encode(&"x\n".repeat(20001), Encoding::Utf8).is_err());
        assert!(has_markers("valid\r\n<<<<<<< branch\ninvalid"));
        assert!(!has_markers("literal <<<<<<< inside a line\n"));
    }
    #[test]
    fn byte_limit_includes_bom_and_saved_encoding() {
        let line = format!("{}\n", "x".repeat(4095));
        let exact = line.repeat(128);
        assert_eq!(encode(&exact, Encoding::Utf8).unwrap().len(), MAX_BYTES);
        assert!(encode(&exact, Encoding::Utf8Bom).is_err());
        assert!(encode(&exact, Encoding::Utf16LittleEndianBom).is_err());
        let utf16_exact = &exact[..MAX_BYTES / 2 - 1];
        assert_eq!(
            encode(utf16_exact, Encoding::Utf16LittleEndianBom)
                .unwrap()
                .len(),
            MAX_BYTES
        );
    }
}
