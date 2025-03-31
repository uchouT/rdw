use std::{
    str::{from_utf8, Utf8Error},
    string::FromUtf16Error,
};

pub(crate) fn string_from_utf16(data: &[u8]) -> Result<String, FromUtf16Error> {
    let utf16: Vec<u16> = data
        .chunks_exact(2)
        .map(|a| u16::from_ne_bytes([a[0], a[1]]))
        .collect();
    String::from_utf16(&utf16)
}

pub(crate) fn utf16_from_utf8(data: &[u8]) -> Result<Vec<u8>, Utf8Error> {
    let utf8 = from_utf8(data)?;
    let utf16 = utf8.encode_utf16().flat_map(u16::to_ne_bytes).collect();
    Ok(utf16)
}
