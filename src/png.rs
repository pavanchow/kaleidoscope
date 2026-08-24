//! A tiny, dependency-free PNG encoder. It emits a valid 8-bit RGBA PNG using
//! stored (uncompressed) DEFLATE blocks inside a zlib stream, so the output
//! opens in any native image viewer without needing an image library or an
//! external converter. It is not small on disk, it trades compression for a
//! readable, self-contained encoder, which fits this project.

use crate::paint::Canvas;

pub fn encode(canvas: &Canvas) -> Vec<u8> {
    let (w, h) = (canvas.width, canvas.height);

    // Raw image data with a per-scanline filter byte (0 = no filter).
    let mut raw = Vec::with_capacity(h * (1 + w * 4));
    for y in 0..h {
        raw.push(0u8);
        for x in 0..w {
            let px = canvas.pixels[y * w + x];
            raw.extend_from_slice(&px);
        }
    }

    let mut out = Vec::new();
    out.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]); // PNG signature

    // IHDR
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(6); // color type 6 = RGBA
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace
    write_chunk(&mut out, b"IHDR", &ihdr);

    // IDAT: a zlib stream wrapping stored DEFLATE blocks.
    let idat = zlib_stored(&raw);
    write_chunk(&mut out, b"IDAT", &idat);

    write_chunk(&mut out, b"IEND", &[]);
    out
}

fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut z = Vec::new();
    z.push(0x78); // CMF
    z.push(0x01); // FLG, chosen so (CMF*256 + FLG) % 31 == 0
    let mut i = 0;
    while i < data.len() || data.is_empty() {
        let remaining = data.len() - i;
        let block = remaining.min(0xFFFF);
        let is_last = i + block >= data.len();
        z.push(if is_last { 1 } else { 0 }); // BFINAL, BTYPE = 00 (stored)
        z.extend_from_slice(&(block as u16).to_le_bytes());
        z.extend_from_slice(&(!(block as u16)).to_le_bytes());
        z.extend_from_slice(&data[i..i + block]);
        i += block;
        if is_last {
            break;
        }
    }
    z.extend_from_slice(&adler32(data).to_be_bytes());
    z
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::Color;

    #[test]
    fn encodes_a_valid_png_header_and_iend() {
        let canvas = Canvas::new(2, 2, Color::rgb(10, 20, 30));
        let png = encode(&canvas);
        assert_eq!(&png[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        // IHDR chunk type sits right after the 8-byte signature and 4-byte length.
        assert_eq!(&png[12..16], b"IHDR");
        // The stream ends with a zero-length IEND chunk and its CRC.
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }

    #[test]
    fn known_crc32_and_adler32_vectors() {
        // CRC32 of "IEND" is a fixed, well-known value.
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        // Adler32 of "abc" is 0x024D0127.
        assert_eq!(adler32(b"abc"), 0x024D_0127);
    }
}
