//! IO abstractions and stream filter decoding.
//!
//! Maps to Java PDFBox `org.apache.pdfbox.filter`.
//!
//! | Filter | Status |
//! |---|---|
//! | `FlateDecode` (zlib/deflate) | ✅ pure Rust |
//! | `ASCIIHexDecode` | ✅ |
//! | `ASCII85Decode` | ✅ |
//! | `RunLengthDecode` | ✅ |
//! | `LZWDecode` | ✅ (post-v1) |
//! | `CCITTFaxDecode` / `DCTDecode` | 🔲 stub |

pub mod lzw;

use crate::cos::CosObject;
use lzw::LzwDecoder;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Decodes a stream's raw bytes according to the `/Filter` entry.
/// Chained filters (array) are applied left-to-right per PDF §7.3.8.1.
pub fn decode_stream(data: &[u8], filter: Option<&CosObject>) -> Result<Vec<u8>, FilterError> {
    let Some(filter) = filter else { return Ok(data.to_vec()); };
    match filter {
        CosObject::Name(name) => apply_filter(data, name.as_bytes()),
        CosObject::Array(filters) => {
            let mut current = data.to_vec();
            for f in filters {
                if let CosObject::Name(name) = f {
                    current = apply_filter(&current, name.as_bytes())?;
                }
            }
            Ok(current)
        }
        _ => Ok(data.to_vec()),
    }
}

fn apply_filter(data: &[u8], name: &[u8]) -> Result<Vec<u8>, FilterError> {
    match name {
        b"FlateDecode" | b"Fl" => flate_decode(data),
        b"ASCIIHexDecode" | b"AHx" => ascii_hex_decode(data),
        b"ASCII85Decode" | b"A85" => ascii85_decode(data),
        b"RunLengthDecode" | b"RL" => run_length_decode(data),
        b"LZWDecode" | b"LZW" => {
            LzwDecoder::decode(data)
                .map_err(|e| FilterError::DecodeFailed(format!("LZW: {}", e)))
        },
        b"CCITTFaxDecode" | b"CCF"
        | b"DCTDecode" | b"DCT" | b"JPXDecode" | b"Crypt" => Ok(data.to_vec()),
        _ => Err(FilterError::UnknownFilter(String::from_utf8_lossy(name).into_owned())),
    }
}

// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterError {
    UnknownFilter(String),
    DecodeFailed(String),
}

impl std::fmt::Display for FilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownFilter(n) => write!(f, "unknown PDF stream filter: {n}"),
            Self::DecodeFailed(msg) => write!(f, "stream decode failed: {msg}"),
        }
    }
}
impl std::error::Error for FilterError {}

// ---------------------------------------------------------------------------
// FlateDecode — pure-Rust zlib/deflate
// ---------------------------------------------------------------------------

fn flate_decode(data: &[u8]) -> Result<Vec<u8>, FilterError> {
    if data.len() < 2 {
        return Err(FilterError::DecodeFailed("too short for zlib header".into()));
    }
    let cmf = data[0]; let flg = data[1];
    if cmf & 0x0F != 8 {
        return Err(FilterError::DecodeFailed(format!("unsupported zlib CM={}", cmf & 0x0F)));
    }
    if (u16::from(cmf) * 256 + u16::from(flg)) % 31 != 0 {
        return Err(FilterError::DecodeFailed("zlib header checksum failed".into()));
    }
    let payload_start = if (flg & 0x20) != 0 { 6 } else { 2 };
    let deflate_data = &data[payload_start..data.len().saturating_sub(4)];
    inflate(deflate_data).map_err(FilterError::DecodeFailed)
}

fn inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut reader = BitReader::new(data);
    let mut output: Vec<u8> = Vec::with_capacity(data.len() * 4);
    loop {
        let bfinal = reader.read_bits(1)?;
        let btype  = reader.read_bits(2)?;
        match btype {
            0b00 => {
                reader.align_to_byte();
                let len  = reader.read_u16_le()?;
                let nlen = reader.read_u16_le()?;
                if len != !nlen { return Err("LEN/NLEN mismatch".into()); }
                for _ in 0..len { output.push(reader.read_byte()?); }
            }
            0b01 => {
                let (ll, d) = fixed_huffman_trees();
                decode_huffman_block(&mut reader, &ll, &d, &mut output)?;
            }
            0b10 => {
                let (ll, d) = read_dynamic_trees(&mut reader)?;
                decode_huffman_block(&mut reader, &ll, &d, &mut output)?;
            }
            _ => return Err("reserved BTYPE 11".into()),
        }
        if bfinal == 1 { break; }
    }
    Ok(output)
}

struct BitReader<'a> { data: &'a [u8], byte_pos: usize, bit_buf: u32, bits_in_buf: u8 }
impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self { Self { data, byte_pos: 0, bit_buf: 0, bits_in_buf: 0 } }
    fn read_bits(&mut self, n: u8) -> Result<u32, String> {
        while self.bits_in_buf < n {
            if self.byte_pos >= self.data.len() { return Err("unexpected end of deflate stream".into()); }
            self.bit_buf |= (self.data[self.byte_pos] as u32) << self.bits_in_buf;
            self.byte_pos += 1; self.bits_in_buf += 8;
        }
        let val = self.bit_buf & ((1u32 << n) - 1);
        self.bit_buf >>= n; self.bits_in_buf -= n; Ok(val)
    }
    fn align_to_byte(&mut self) { let d = self.bits_in_buf % 8; self.bit_buf >>= d; self.bits_in_buf -= d; }
    fn read_u16_le(&mut self) -> Result<u16, String> {
        Ok(self.read_byte()? as u16 | ((self.read_byte()? as u16) << 8))
    }
    fn read_byte(&mut self) -> Result<u8, String> {
        if self.bits_in_buf >= 8 {
            let b = (self.bit_buf & 0xFF) as u8; self.bit_buf >>= 8; self.bits_in_buf -= 8; Ok(b)
        } else {
            if self.byte_pos >= self.data.len() { return Err("unexpected end (byte)".into()); }
            let b = self.data[self.byte_pos]; self.byte_pos += 1; Ok(b)
        }
    }
}

struct HuffmanTree { entries: Vec<(u8, u16, u32)>, }
impl HuffmanTree {
    fn from_lengths(lengths: &[u8]) -> Self {
        let max_bits = *lengths.iter().max().unwrap_or(&0);
        if max_bits == 0 { return Self { entries: vec![] }; }
        let mut bl_count = vec![0u32; max_bits as usize + 1];
        for &l in lengths { if l > 0 { bl_count[l as usize] += 1; } }
        let mut next_code = vec![0u32; max_bits as usize + 2];
        let mut code = 0u32;
        for bits in 1..=max_bits as usize {
            code = (code + bl_count[bits - 1]) << 1;
            next_code[bits] = code;
        }
        let mut entries = Vec::new();
        for (sym, &len) in lengths.iter().enumerate() {
            if len > 0 {
                entries.push((len, sym as u16, next_code[len as usize]));
                next_code[len as usize] += 1;
            }
        }
        entries.sort_unstable();
        Self { entries }
    }
    fn decode(&self, reader: &mut BitReader) -> Result<u16, String> {
        if self.entries.is_empty() { return Err("empty huffman tree".into()); }
        let mut code = 0u32; let mut bits_read = 0u8;
        for &(len, sym, expected) in &self.entries {
            while bits_read < len { code = (code << 1) | reader.read_bits(1)?; bits_read += 1; }
            if code == expected { return Ok(sym); }
        }
        Err("invalid huffman code".into())
    }
}

fn fixed_huffman_trees() -> (HuffmanTree, HuffmanTree) {
    let mut ll = vec![0u8; 288];
    for i in 0..=143 { ll[i] = 8; } for i in 144..=255 { ll[i] = 9; }
    for i in 256..=279 { ll[i] = 7; } for i in 280..=287 { ll[i] = 8; }
    (HuffmanTree::from_lengths(&ll), HuffmanTree::from_lengths(&vec![5u8; 32]))
}

fn read_dynamic_trees(reader: &mut BitReader) -> Result<(HuffmanTree, HuffmanTree), String> {
    let hlit  = reader.read_bits(5)? as usize + 257;
    let hdist = reader.read_bits(5)? as usize + 1;
    let hclen = reader.read_bits(4)? as usize + 4;
    const CL_ORDER: [usize; 19] = [16,17,18,0,8,7,9,6,10,5,11,4,12,3,13,2,14,1,15];
    let mut cl_lengths = [0u8; 19];
    for i in 0..hclen { cl_lengths[CL_ORDER[i]] = reader.read_bits(3)? as u8; }
    let cl_tree = HuffmanTree::from_lengths(&cl_lengths);
    let total = hlit + hdist;
    let mut lengths = vec![0u8; total];
    let mut i = 0;
    while i < total {
        let sym = cl_tree.decode(reader)?;
        match sym {
            0..=15 => { lengths[i] = sym as u8; i += 1; }
            16 => { let r = reader.read_bits(2)? as usize + 3; let v = if i>0{lengths[i-1]}else{0}; for _ in 0..r { if i<total { lengths[i]=v; i+=1; } } }
            17 => { let r = reader.read_bits(3)? as usize + 3; for _ in 0..r { if i<total { lengths[i]=0; i+=1; } } }
            18 => { let r = reader.read_bits(7)? as usize + 11; for _ in 0..r { if i<total { lengths[i]=0; i+=1; } } }
            _ => return Err(format!("invalid cl sym {sym}")),
        }
    }
    Ok((HuffmanTree::from_lengths(&lengths[..hlit]), HuffmanTree::from_lengths(&lengths[hlit..])))
}

fn decode_huffman_block(reader: &mut BitReader, ll: &HuffmanTree, dist: &HuffmanTree, out: &mut Vec<u8>) -> Result<(), String> {
    const LB: [u16;29] = [3,4,5,6,7,8,9,10,11,13,15,17,19,23,27,31,35,43,51,59,67,83,99,115,131,163,195,227,258];
    const LE: [u8;29]  = [0,0,0,0,0,0,0,0,1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4,5,5,5,5,0];
    const DB: [u16;30] = [1,2,3,4,5,7,9,13,17,25,33,49,65,97,129,193,257,385,513,769,1025,1537,2049,3073,4097,6145,8193,12289,16385,24577];
    const DE: [u8;30]  = [0,0,0,0,1,1,2,2,3,3,4,4,5,5,6,6,7,7,8,8,9,9,10,10,11,11,12,12,13,13];
    loop {
        let sym = ll.decode(reader)?;
        match sym {
            0..=255 => out.push(sym as u8),
            256 => break,
            257..=285 => {
                let li = (sym - 257) as usize;
                let length = LB[li] as usize + reader.read_bits(LE[li])? as usize;
                let di = dist.decode(reader)? as usize;
                if di >= 30 { return Err(format!("invalid dist sym {di}")); }
                let d = DB[di] as usize + reader.read_bits(DE[di])? as usize;
                let start = out.len().checked_sub(d).ok_or_else(|| format!("dist {d} > output"))?;
                for j in 0..length { let b = out[start + j % d]; out.push(b); }
            }
            _ => return Err(format!("invalid ll sym {sym}")),
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ASCIIHexDecode
// ---------------------------------------------------------------------------
fn ascii_hex_decode(data: &[u8]) -> Result<Vec<u8>, FilterError> {
    let mut out = Vec::with_capacity(data.len() / 2);
    let mut nibble: Option<u8> = None;
    for &b in data {
        let val = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            b'>' => break,
            b' '|b'\t'|b'\r'|b'\n' => continue,
            _ => return Err(FilterError::DecodeFailed(format!("invalid AHx byte 0x{b:02X}"))),
        };
        if let Some(hi) = nibble.take() { out.push((hi << 4) | val); } else { nibble = Some(val); }
    }
    if let Some(hi) = nibble { out.push(hi << 4); }
    Ok(out)
}

// ---------------------------------------------------------------------------
// ASCII85Decode
// ---------------------------------------------------------------------------
fn ascii85_decode(data: &[u8]) -> Result<Vec<u8>, FilterError> {
    let mut out = Vec::new();
    let mut group = [0u8; 5]; let mut count = 0usize;
    let mut i = 0;
    while i < data.len() {
        let b = data[i]; i += 1;
        match b {
            b'z' if count == 0 => { out.extend_from_slice(&[0u8; 4]); }
            b'~' => { if i < data.len() && data[i] == b'>' { break; } }
            b' '|b'\t'|b'\r'|b'\n' => {}
            b'!'..=b'u' => {
                group[count] = b - b'!'; count += 1;
                if count == 5 {
                    let v = group[0] as u32*52200625 + group[1] as u32*614125 + group[2] as u32*7225 + group[3] as u32*85 + group[4] as u32;
                    out.extend_from_slice(&v.to_be_bytes()); count = 0;
                }
            }
            _ => return Err(FilterError::DecodeFailed(format!("invalid A85 byte 0x{b:02X}"))),
        }
    }
    if count > 0 {
        for j in count..5 { group[j] = b'u' - b'!'; }
        let v = group[0] as u32*52200625 + group[1] as u32*614125 + group[2] as u32*7225 + group[3] as u32*85 + group[4] as u32;
        out.extend_from_slice(&v.to_be_bytes()[..count-1]);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// RunLengthDecode
// ---------------------------------------------------------------------------
fn run_length_decode(data: &[u8]) -> Result<Vec<u8>, FilterError> {
    let mut out = Vec::new(); let mut i = 0;
    while i < data.len() {
        let b = data[i] as i16; i += 1;
        match b {
            128 => break,
            0..=127 => {
                let n = b as usize + 1;
                if i + n > data.len() { return Err(FilterError::DecodeFailed("truncated RL literal".into())); }
                out.extend_from_slice(&data[i..i+n]); i += n;
            }
            _ => {
                let n = (257 - b) as usize;
                if i >= data.len() { return Err(FilterError::DecodeFailed("truncated RL repeat".into())); }
                let byte = data[i]; i += 1;
                for _ in 0..n { out.push(byte); }
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::CosName;

    fn make_stored_zlib(data: &[u8]) -> Vec<u8> {
        let cmf: u8 = 0x78;
        let rem = (cmf as u16 * 256) % 31;
        let flg: u8 = if rem == 0 { 0x01 } else { (31 - rem) as u8 };
        let mut out = vec![cmf, flg];
        out.push(0x01); // BFINAL=1 BTYPE=00
        let len = data.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(data);
        let (mut s1, mut s2) = (1u32, 0u32);
        for &b in data { s1 = (s1 + b as u32) % 65521; s2 = (s2 + s1) % 65521; }
        out.extend_from_slice(&((s2 << 16) | s1).to_be_bytes());
        out
    }

    #[test] fn ascii_hex_basic() { assert_eq!(ascii_hex_decode(b"48656c6c6f>").unwrap(), b"Hello"); }
    #[test] fn ascii_hex_whitespace() { assert_eq!(ascii_hex_decode(b"48 65 6c 6c 6f>").unwrap(), b"Hello"); }
    #[test] fn ascii_hex_odd_nibble() { assert_eq!(ascii_hex_decode(b"F>").unwrap(), &[0xF0]); }
    #[test] fn ascii_hex_empty() { assert!(ascii_hex_decode(b">").unwrap().is_empty()); }

    #[test] fn ascii85_zeros() { assert_eq!(ascii85_decode(b"z~>").unwrap(), &[0u8;4]); }
    #[test] fn ascii85_hello() { assert_eq!(&ascii85_decode(b"87cURDZ~>").unwrap()[..5], b"Hello"); }
    #[test] fn ascii85_empty() { assert!(ascii85_decode(b"~>").unwrap().is_empty()); }

    #[test] fn run_length_literal() { assert_eq!(run_length_decode(&[2,b'A',b'B',b'C',128]).unwrap(), b"ABC"); }
    #[test] fn run_length_repeat() { assert_eq!(run_length_decode(&[253,b'X',128]).unwrap(), b"XXXX"); }
    #[test] fn run_length_eod() { assert!(run_length_decode(&[128]).unwrap().is_empty()); }

    #[test] fn flate_stored_roundtrip() {
        let orig = b"Hello, FlateDecode!";
        assert_eq!(flate_decode(&make_stored_zlib(orig)).unwrap(), orig);
    }
    #[test] fn flate_empty_stored() { assert!(flate_decode(&make_stored_zlib(b"")).unwrap().is_empty()); }
    #[test] fn flate_bad_header() { assert!(flate_decode(&[0x00,0x00,0x01]).is_err()); }

    #[test] fn decode_stream_passthrough() { assert_eq!(decode_stream(b"raw",None).unwrap(), b"raw"); }
    #[test] fn decode_stream_ahx() {
        let f = CosObject::Name(CosName::new(b"ASCIIHexDecode".to_vec()));
        assert_eq!(decode_stream(b"48656c6c6f>",Some(&f)).unwrap(), b"Hello");
    }
    #[test] fn decode_stream_array() {
        let f = CosObject::Array(vec![CosObject::Name(CosName::new(b"ASCIIHexDecode".to_vec()))]);
        assert_eq!(decode_stream(b"48656c6c6f>",Some(&f)).unwrap(), b"Hello");
    }
    #[test] fn decode_stream_unknown_err() {
        let f = CosObject::Name(CosName::new(b"Bogus".to_vec()));
        assert!(matches!(decode_stream(b"x",Some(&f)), Err(FilterError::UnknownFilter(_))));
    }
}
