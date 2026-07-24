//! PDF Stream Decoder Implementations (DCT, LZW, Flate, etc.)

use crate::PdfError;
use jpeg_decoder::Decoder as JpegDecoder;
use weezl::decode::Decoder as LzwDecoder;
use weezl::BitOrder;

/// Decodes stream data based on the applied PDF Filter.
pub fn decode_stream_filters(raw_data: &[u8], filter_names: &[String]) -> Result<Vec<u8>, PdfError> {
    let mut current_data = raw_data.to_vec();

    // PDF filters are applied in order. We must decode them in reverse.
    for filter in filter_names.iter().rev() {
        current_data = match filter.as_str() {
            "FlateDecode" | "Fl" => {
                crate::compress::flate::decode(&current_data).map_err(|e| PdfError::Parse {
                    offset: None,
                    context: format!("FlateDecode failed: {}", e),
                })?
            }
            "LZWDecode" | "LZW" => {
                let mut lzw = LzwDecoder::new(BitOrder::Msb, 8);
                lzw.decode(&current_data).map_err(|e| PdfError::Parse {
                    offset: None,
                    context: format!("LZWDecode failed: {:?}", e),
                })?
            }
            "DCTDecode" | "DCT" => {
                let mut decoder = JpegDecoder::new(std::io::Cursor::new(&current_data));
                decoder.decode().map_err(|e| PdfError::Parse {
                    offset: None,
                    context: format!("DCTDecode (JPEG) failed: {}", e),
                })?
            }
            "ASCIIHexDecode" | "AHx" => {
                decode_ascii_hex(&current_data)?
            }
            "CCITTFaxDecode" | "JPXDecode" | "JBIG2Decode" => {
                return Err(PdfError::Unsupported {
                    feature: "CCITTFax, JPX, and JBIG2 decoders are stubbed (Requires robust external C-libs).",
                });
            }
            _ => current_data, // Unrecognized filter, pass-through
        };
    }

    Ok(current_data)
}

fn decode_ascii_hex(data: &[u8]) -> Result<Vec<u8>, PdfError> {
    // Basic implementation of ASCII Hex
    let mut out = Vec::with_capacity(data.len() / 2);
    let mut current_byte = 0u8;
    let mut high_nibble = true;

    for &b in data {
        if b == b'>' {
            if !high_nibble { out.push(current_byte << 4); }
            break;
        }
        if b.is_ascii_whitespace() { continue; }
        
        let val = match b {
            b'0'..=b'9' => b - b'0',
            b'A'..=b'F' => b - b'A' + 10,
            b'a'..=b'f' => b - b'a' + 10,
            _ => return Err(PdfError::Parse { offset: None, context: "Invalid ASCIIHex string".to_string() }),
        };

        if high_nibble {
            current_byte = val;
            high_nibble = false;
        } else {
            out.push((current_byte << 4) | val);
            high_nibble = true;
        }
    }
    Ok(out)
}