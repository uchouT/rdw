use std::{
    iter::once,
    str::{from_utf8, Utf8Error},
    string::FromUtf16Error,
};

/// Formats a string from the RDP ClipboardFormatId CF_UNICODETEXT
pub(crate) fn string_from_utf16(data: &[u8]) -> Result<String, FromUtf16Error> {
    let utf16: Vec<u16> = data
        .chunks_exact(2)
        .map(|a| u16::from_le_bytes([a[0], a[1]]))
        .collect();
    String::from_utf16(&utf16)
}

/// Formats a string for the RDP ClipboardFormatId CF_UNICODETEXT
pub(crate) fn utf16_from_utf8(data: &[u8]) -> Result<Vec<u8>, Utf8Error> {
    let utf8 = from_utf8(data)?;
    let utf16 = utf8
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .chain(once(0))
        .collect();
    Ok(utf16)
}
