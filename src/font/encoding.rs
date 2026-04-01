//! Font encoding — `PDFEncoding`.
//!
//! PDF §9.6.5 — encodes single-byte character codes to glyph names / Unicode.
//!
//! Maps to Java PDFBox `Encoding`, `WinAnsiEncoding`, `MacRomanEncoding`,
//! `StandardEncoding`, `PDFDocEncoding`.

use crate::cos::{CosDictionary, CosName, CosObject};

// ---------------------------------------------------------------------------
// Encoding variant
// ---------------------------------------------------------------------------

/// Identifies the base encoding referenced by name.
///
/// Corresponds to the optional `/BaseEncoding` key inside an encoding dict,
/// or the font's `/Encoding` entry when it is a name rather than a dict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaseEncoding {
    /// Adobe standard encoding (default for Type1 fonts that don't declare one).
    StandardEncoding,
    /// MacRomanEncoding.
    MacRomanEncoding,
    /// WinAnsiEncoding (Windows Latin-1 code page 1252).
    WinAnsiEncoding,
    /// PDFDocEncoding (superset of Latin-1 used in PDF strings).
    PdfDocEncoding,
    /// FontSpecific (symbolic fonts — codes map directly to glyph IDs).
    FontSpecific,
}

impl BaseEncoding {
    pub fn from_name(name: &[u8]) -> Option<Self> {
        match name {
            b"StandardEncoding"  => Some(Self::StandardEncoding),
            b"MacRomanEncoding"  => Some(Self::MacRomanEncoding),
            b"WinAnsiEncoding"   => Some(Self::WinAnsiEncoding),
            b"PDFDocEncoding"    => Some(Self::PdfDocEncoding),
            b"FontSpecific"      => Some(Self::FontSpecific),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StandardEncoding  => "StandardEncoding",
            Self::MacRomanEncoding  => "MacRomanEncoding",
            Self::WinAnsiEncoding   => "WinAnsiEncoding",
            Self::PdfDocEncoding    => "PDFDocEncoding",
            Self::FontSpecific      => "FontSpecific",
        }
    }
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

/// A resolved 256-slot single-byte encoding (code → Unicode scalar).
///
/// Slot 0 is always mapped to U+FFFD (undefined). Missing slots also map
/// to U+FFFD. This matches the behaviour of Java PDFBox's `Encoding`.
#[derive(Debug, Clone)]
pub struct Encoding {
    /// Base encoding name (informational).
    pub base: Option<BaseEncoding>,
    /// Per-code Unicode mapping; index = byte code 0–255.
    pub table: [char; 256],
    /// Per-code PDF glyph name (from `/Differences`); None = use base.
    pub glyph_names: [Option<String>; 256],
}

const UNDEF: char = '\u{FFFD}';

impl Encoding {
    // -----------------------------------------------------------------------
    // Named base-encoding constructors
    // -----------------------------------------------------------------------

    /// WinAnsiEncoding — Windows code page 1252.
    pub fn win_ansi() -> Self {
        let mut table = [UNDEF; 256];
        // 0x20–0x7E and 0xA0–0xFF are direct Unicode (Latin-1 supplement)
        for b in 0x20u8..=0x7Eu8 { table[b as usize] = b as char; }
        for b in 0xA0u8..=0xFFu8 { table[b as usize] = b as char; }
        // Windows-1252 extra characters in 0x80–0x9F (selected subset)
        table[0x80] = '\u{20AC}'; // €
        table[0x82] = '\u{201A}'; // ‚
        table[0x83] = '\u{0192}'; // ƒ
        table[0x84] = '\u{201E}'; // „
        table[0x85] = '\u{2026}'; // …
        table[0x86] = '\u{2020}'; // †
        table[0x87] = '\u{2021}'; // ‡
        table[0x88] = '\u{02C6}'; // ˆ
        table[0x89] = '\u{2030}'; // ‰
        table[0x8A] = '\u{0160}'; // Š
        table[0x8B] = '\u{2039}'; // ‹
        table[0x8C] = '\u{0152}'; // Œ
        table[0x8E] = '\u{017D}'; // Ž
        table[0x91] = '\u{2018}'; // '
        table[0x92] = '\u{2019}'; // '
        table[0x93] = '\u{201C}'; // "
        table[0x94] = '\u{201D}'; // "
        table[0x95] = '\u{2022}'; // •
        table[0x96] = '\u{2013}'; // –
        table[0x97] = '\u{2014}'; // —
        table[0x98] = '\u{02DC}'; // ˜
        table[0x99] = '\u{2122}'; // ™
        table[0x9A] = '\u{0161}'; // š
        table[0x9B] = '\u{203A}'; // ›
        table[0x9C] = '\u{0153}'; // œ
        table[0x9E] = '\u{017E}'; // ž
        table[0x9F] = '\u{0178}'; // Ÿ
        Self { base: Some(BaseEncoding::WinAnsiEncoding), table, glyph_names: std::array::from_fn(|_| None) }
    }

    /// MacRomanEncoding.
    pub fn mac_roman() -> Self {
        let mut table = [UNDEF; 256];
        for b in 0x20u8..=0x7Eu8 { table[b as usize] = b as char; }
        // Core Mac Roman block (0x80–0xFF)
        const MAC_ROMAN_HIGH: [char; 128] = [
            'Ä','Å','Ç','É','Ñ','Ö','Ü','á','à','â','ä','ã','å','ç','é','è',
            'ê','ë','í','ì','î','ï','ñ','ó','ò','ô','ö','õ','ú','ù','û','ü',
            '†','°','¢','£','§','•','¶','ß','®','©','™','´','¨','\u{2260}','Æ','Ø',
            '\u{221E}','±','\u{2264}','\u{2265}','¥','µ','\u{2202}','\u{2211}','\u{220F}','π','\u{222B}','ª','º','\u{03A9}','æ','ø',
            '¿','¡','¬','\u{221A}','\u{0192}','\u{2248}','\u{2206}','«','»','\u{2026}','\u{00A0}','À','Ã','Õ','Œ','œ',
            '\u{2013}','\u{2014}','\u{201C}','\u{201D}','\u{2018}','\u{2019}','\u{00F7}','\u{25CA}','ÿ','\u{0178}','\u{2044}','\u{20AC}','\u{2039}','\u{203A}','\u{FB01}','\u{FB02}',
            '\u{2021}','·','\u{201A}','\u{201E}','\u{2030}','Â','Ê','Á','Ë','È','Í','Î','Ï','Ì','Ó','Ô',
            '\u{F8FF}','Ò','Ú','Û','Ù','ı','\u{02C6}','\u{02DC}','\u{00AF}','\u{02D8}','\u{02D9}','\u{02DA}','\u{00B8}','\u{02DD}','\u{02DB}','\u{02C7}',
        ];
        for (i, &c) in MAC_ROMAN_HIGH.iter().enumerate() {
            table[0x80 + i] = c;
        }
        Self { base: Some(BaseEncoding::MacRomanEncoding), table, glyph_names: std::array::from_fn(|_| None) }
    }

    /// StandardEncoding (Adobe, used by many Type1 fonts).
    pub fn standard() -> Self {
        let mut table = [UNDEF; 256];
        // Standard encoding is sparse — map printable ASCII directly
        for b in 0x21u8..=0x7Eu8 { table[b as usize] = b as char; }
        // Notable differences from Latin-1
        table[0x60] = '\u{2018}'; // ` → left single quotation
        table[0x27] = '\u{2019}'; // ' → right single quotation
        table[0xA4] = '\u{2044}'; // /fraction
        table[0xA6] = '\u{0192}'; // /florin
        Self { base: Some(BaseEncoding::StandardEncoding), table, glyph_names: std::array::from_fn(|_| None) }
    }

    /// FontSpecific — pass-through (code equals glyph ID).
    pub fn font_specific() -> Self {
        let mut table = [UNDEF; 256];
        for b in 0u8..=0xFFu8 {
            table[b as usize] = char::from_u32(b as u32).unwrap_or(UNDEF);
        }
        Self { base: Some(BaseEncoding::FontSpecific), table, glyph_names: std::array::from_fn(|_| None) }
    }

    // -----------------------------------------------------------------------
    // Parse from COS
    // -----------------------------------------------------------------------

    /// Parse an encoding from a PDF `/Encoding` entry.
    ///
    /// The entry can be:
    /// - A `/Name` (e.g. `/WinAnsiEncoding`) → use that base encoding.
    /// - A dictionary with optional `/BaseEncoding` + `/Differences` array.
    pub fn from_cos(obj: &CosObject) -> Self {
        match obj {
            CosObject::Name(n) => {
                match BaseEncoding::from_name(n.as_bytes()) {
                    Some(BaseEncoding::WinAnsiEncoding)  => Self::win_ansi(),
                    Some(BaseEncoding::MacRomanEncoding) => Self::mac_roman(),
                    Some(BaseEncoding::StandardEncoding) => Self::standard(),
                    Some(BaseEncoding::FontSpecific) | None => Self::font_specific(),
                    Some(BaseEncoding::PdfDocEncoding)   => Self::win_ansi(), // close enough
                }
            }
            CosObject::Dictionary(dict) => Self::from_encoding_dict(dict),
            _ => Self::win_ansi(), // default fallback
        }
    }

    /// Parse an encoding dictionary with optional BaseEncoding + Differences.
    pub fn from_encoding_dict(dict: &CosDictionary) -> Self {
        // Start with the base encoding
        let mut enc = match dict.get(&CosName::new(b"BaseEncoding".to_vec()))
            .and_then(|v| v.as_name())
            .and_then(|n| BaseEncoding::from_name(n.as_bytes()))
        {
            Some(BaseEncoding::WinAnsiEncoding)  => Self::win_ansi(),
            Some(BaseEncoding::MacRomanEncoding) => Self::mac_roman(),
            Some(BaseEncoding::StandardEncoding) => Self::standard(),
            _ => Self::win_ansi(),
        };

        // Apply /Differences array: [ code /GlyphName /GlyphName ... code ... ]
        if let Some(diffs) = dict.get_array(&CosName::new(b"Differences".to_vec())) {
            let mut current_code: u32 = 0;
            for item in diffs {
                match item {
                    CosObject::Integer(n) => { current_code = *n as u32; }
                    CosObject::Name(name) => {
                        if current_code < 256 {
                            let glyph = String::from_utf8_lossy(name.as_bytes()).to_string();
                            enc.glyph_names[current_code as usize] = Some(glyph.clone());
                            // Map common glyph names to Unicode
                            if let Some(c) = glyph_name_to_char(&glyph) {
                                enc.table[current_code as usize] = c;
                            }
                        }
                        current_code += 1;
                    }
                    _ => {}
                }
            }
        }

        enc
    }

    // -----------------------------------------------------------------------
    // Accessors
    // -----------------------------------------------------------------------

    /// Decode a single byte to a `char` using this encoding.
    /// Returns `\u{FFFD}` (replacement character) for unmapped codes.
    pub fn decode_byte(&self, byte: u8) -> char {
        self.table[byte as usize]
    }

    /// Decode a byte slice to a `String`.
    pub fn decode_bytes(&self, bytes: &[u8]) -> String {
        bytes.iter().map(|&b| self.decode_byte(b)).collect()
    }

    /// Returns the glyph name for a code (from /Differences), if set.
    pub fn glyph_name(&self, code: u8) -> Option<&str> {
        self.glyph_names[code as usize].as_deref()
    }
}

// ---------------------------------------------------------------------------
// Glyph-name → Unicode mapping (PDF Glyph List subset)
// ---------------------------------------------------------------------------

/// Map an Adobe glyph name to a Unicode char.
/// Covers the most common names from the Adobe Glyph List (AGL).
pub fn glyph_name_to_char(name: &str) -> Option<char> {
    // Full AGL has ~4000 entries; we include the most common ~300.
    // For names of the form "uniXXXX" or "uXXXXX" use the numeric form.
    if let Some(hex) = name.strip_prefix("uni") {
        if hex.len() == 4 {
            return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
        }
    }
    if let Some(hex) = name.strip_prefix('u') {
        if (4..=6).contains(&hex.len()) {
            return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
        }
    }

    // Static AGL subset — sorted for binary-search could be faster, but
    // this is called only during font loading (not per-character), so a
    // match is fine.
    Some(match name {
        "A"=>'A',"B"=>'B',"C"=>'C',"D"=>'D',"E"=>'E',"F"=>'F',"G"=>'G',"H"=>'H',
        "I"=>'I',"J"=>'J',"K"=>'K',"L"=>'L',"M"=>'M',"N"=>'N',"O"=>'O',"P"=>'P',
        "Q"=>'Q',"R"=>'R',"S"=>'S',"T"=>'T',"U"=>'U',"V"=>'V',"W"=>'W',"X"=>'X',
        "Y"=>'Y',"Z"=>'Z',
        "a"=>'a',"b"=>'b',"c"=>'c',"d"=>'d',"e"=>'e',"f"=>'f',"g"=>'g',"h"=>'h',
        "i"=>'i',"j"=>'j',"k"=>'k',"l"=>'l',"m"=>'m',"n"=>'n',"o"=>'o',"p"=>'p',
        "q"=>'q',"r"=>'r',"s"=>'s',"t"=>'t',"u"=>'u',"v"=>'v',"w"=>'w',"x"=>'x',
        "y"=>'y',"z"=>'z',
        "zero"=>'0',"one"=>'1',"two"=>'2',"three"=>'3',"four"=>'4',
        "five"=>'5',"six"=>'6',"seven"=>'7',"eight"=>'8',"nine"=>'9',
        "space"=>' ',"exclam"=>'!',"quotedbl"=>'"',"numbersign"=>'#',
        "dollar"=>'$',"percent"=>'%',"ampersand"=>'&',"quotesingle"=>'\'',
        "parenleft"=>'(',"parenright"=>')',"asterisk"=>'*',"plus"=>'+',
        "comma"=>',',"hyphen"=>'-',"period"=>'.',"slash"=>'/',
        "colon"=>':',"semicolon"=>';',"less"=>'<',"equal"=>'=',
        "greater"=>'>',"question"=>'?',"at"=>'@',
        "bracketleft"=>'[',"backslash"=>'\\',"bracketright"=>']',
        "asciicircum"=>'^',"underscore"=>'_',"grave"=>'`',
        "braceleft"=>'{',"bar"=>'|',"braceright"=>'}',"asciitilde"=>'~',
        "exclamdown"=>'¡',"cent"=>'¢',"sterling"=>'£',"currency"=>'¤',
        "yen"=>'¥',"brokenbar"=>'¦',"section"=>'§',"dieresis"=>'¨',
        "copyright"=>'©',"ordfeminine"=>'ª',"guillemotleft"=>'«',
        "logicalnot"=>'¬',"registered"=>'®',"macron"=>'¯',
        "degree"=>'°',"plusminus"=>'±',"twosuperior"=>'²',
        "threesuperior"=>'³',"acute"=>'´',"mu"=>'µ',"paragraph"=>'¶',
        "periodcentered"=>'·',"cedilla"=>'¸',"onesuperior"=>'¹',
        "ordmasculine"=>'º',"guillemotright"=>'»',
        "onequarter"=>'¼',"onehalf"=>'½',"threequarters"=>'¾',
        "questiondown"=>'¿',
        "Agrave"=>'À',"Aacute"=>'Á',"Acircumflex"=>'Â',"Atilde"=>'Ã',
        "Adieresis"=>'Ä',"Aring"=>'Å',"AE"=>'Æ',"Ccedilla"=>'Ç',
        "Egrave"=>'È',"Eacute"=>'É',"Ecircumflex"=>'Ê',"Edieresis"=>'Ë',
        "Igrave"=>'Ì',"Iacute"=>'Í',"Icircumflex"=>'Î',"Idieresis"=>'Ï',
        "Eth"=>'Ð',"Ntilde"=>'Ñ',"Ograve"=>'Ò',"Oacute"=>'Ó',
        "Ocircumflex"=>'Ô',"Otilde"=>'Õ',"Odieresis"=>'Ö',"multiply"=>'×',
        "Oslash"=>'Ø',"Ugrave"=>'Ù',"Uacute"=>'Ú',"Ucircumflex"=>'Û',
        "Udieresis"=>'Ü',"Yacute"=>'Ý',"Thorn"=>'Þ',"germandbls"=>'ß',
        "agrave"=>'à',"aacute"=>'á',"acircumflex"=>'â',"atilde"=>'ã',
        "adieresis"=>'ä',"aring"=>'å',"ae"=>'æ',"ccedilla"=>'ç',
        "egrave"=>'è',"eacute"=>'é',"ecircumflex"=>'ê',"edieresis"=>'ë',
        "igrave"=>'ì',"iacute"=>'í',"icircumflex"=>'î',"idieresis"=>'ï',
        "eth"=>'ð',"ntilde"=>'ñ',"ograve"=>'ò',"oacute"=>'ó',
        "ocircumflex"=>'ô',"otilde"=>'õ',"odieresis"=>'ö',"divide"=>'÷',
        "oslash"=>'ø',"ugrave"=>'ù',"uacute"=>'ú',"ucircumflex"=>'û',
        "udieresis"=>'ü',"yacute"=>'ý',"thorn"=>'þ',"ydieresis"=>'ÿ',
        // Typography specials
        "fi"=>'\u{FB01}',"fl"=>'\u{FB02}',
        "endash"=>'\u{2013}',"emdash"=>'\u{2014}',
        "quotesinglbase"=>'\u{201A}',"quotedblbase"=>'\u{201E}',
        "quotedblleft"=>'\u{201C}',"quotedblright"=>'\u{201D}',
        "quoteleft"=>'\u{2018}',"quoteright"=>'\u{2019}',
        "bullet"=>'\u{2022}',"ellipsis"=>'\u{2026}',
        "dagger"=>'\u{2020}',"daggerdbl"=>'\u{2021}',
        "florin"=>'\u{0192}',"fraction"=>'\u{2044}',
        "perthousand"=>'\u{2030}',"guilsinglleft"=>'\u{2039}',
        "guilsinglright"=>'\u{203A}',"Euro"=>'\u{20AC}',
        "trademark"=>'\u{2122}',"OE"=>'\u{0152}',"oe"=>'\u{0153}',
        "Scaron"=>'\u{0160}',"scaron"=>'\u{0161}',
        "Zcaron"=>'\u{017D}',"zcaron"=>'\u{017E}',
        "Ydieresis"=>'\u{0178}',
        "circumflex"=>'\u{02C6}',"tilde"=>'\u{02DC}',
        "dotlessi"=>'\u{0131}',
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::{CosName, CosObject};

    #[test]
    fn win_ansi_ascii_range() {
        let enc = Encoding::win_ansi();
        assert_eq!(enc.decode_byte(b'A'), 'A');
        assert_eq!(enc.decode_byte(b'z'), 'z');
        assert_eq!(enc.decode_byte(b' '), ' ');
    }

    #[test]
    fn win_ansi_extended() {
        let enc = Encoding::win_ansi();
        assert_eq!(enc.decode_byte(0x80), '\u{20AC}'); // Euro sign
        assert_eq!(enc.decode_byte(0xE9), 'é');
    }

    #[test]
    fn mac_roman_high_range() {
        let enc = Encoding::mac_roman();
        assert_eq!(enc.decode_byte(0x80), 'Ä');
    }

    #[test]
    fn standard_encoding_backtick() {
        let enc = Encoding::standard();
        assert_eq!(enc.decode_byte(0x60), '\u{2018}'); // left single quote
    }

    #[test]
    fn base_encoding_from_name() {
        assert_eq!(BaseEncoding::from_name(b"WinAnsiEncoding"), Some(BaseEncoding::WinAnsiEncoding));
        assert_eq!(BaseEncoding::from_name(b"Unknown"), None);
    }

    #[test]
    fn base_encoding_as_str() {
        assert_eq!(BaseEncoding::WinAnsiEncoding.as_str(), "WinAnsiEncoding");
    }

    #[test]
    fn encoding_from_cos_name() {
        let obj = CosObject::Name(CosName::new(b"WinAnsiEncoding".to_vec()));
        let enc = Encoding::from_cos(&obj);
        assert_eq!(enc.decode_byte(b'A'), 'A');
        assert_eq!(enc.base, Some(BaseEncoding::WinAnsiEncoding));
    }

    #[test]
    fn encoding_differences_override() {
        let mut dict = CosDictionary::new();
        dict.set(
            CosName::new(b"BaseEncoding".to_vec()),
            CosObject::Name(CosName::new(b"WinAnsiEncoding".to_vec())),
        );
        // Map code 0x41 ('A') to 'bullet' (•)
        dict.set(
            CosName::new(b"Differences".to_vec()),
            CosObject::Array(vec![
                CosObject::Integer(0x41),
                CosObject::Name(CosName::new(b"bullet".to_vec())),
            ]),
        );
        let enc = Encoding::from_encoding_dict(&dict);
        assert_eq!(enc.decode_byte(0x41), '\u{2022}'); // bullet
        assert_eq!(enc.glyph_name(0x41), Some("bullet"));
    }

    #[test]
    fn glyph_name_to_char_letters() {
        assert_eq!(glyph_name_to_char("A"), Some('A'));
        assert_eq!(glyph_name_to_char("eacute"), Some('é'));
        assert_eq!(glyph_name_to_char("Euro"), Some('\u{20AC}'));
    }

    #[test]
    fn glyph_name_to_char_uni_prefix() {
        assert_eq!(glyph_name_to_char("uni0041"), Some('A'));
    }

    #[test]
    fn glyph_name_to_char_u_prefix() {
        assert_eq!(glyph_name_to_char("u0041"), Some('A'));
    }

    #[test]
    fn glyph_name_unknown_returns_none() {
        assert_eq!(glyph_name_to_char("xyzzy"), None);
    }

    #[test]
    fn decode_bytes_string() {
        let enc = Encoding::win_ansi();
        assert_eq!(enc.decode_bytes(b"Hello"), "Hello");
    }
}

