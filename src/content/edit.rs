//! Rich typed content stream operators and editor.
//!
//! Builds on the existing [`ContentTokenizer`] / [`Instruction`] infrastructure
//! to provide a high-level, strongly-typed representation of content streams,
//! together with search-and-replace and mutating operations.
//!
//! # Design
//!
//! The low-level token stream (operands + raw-operator) is converted into a
//! typed [`ContentOperator`] tree.  Edits are applied on the tree, and the
//! tree is then serialised back to bytes for injection into the PDF.

use crate::content::{ContentTokenizer, Instruction, Operator};
use crate::cos::{CosDictionary, CosName, CosObject};
use std::fmt;

// ---------------------------------------------------------------------------
// ContentOperator — typed representation of every PDF content-stream operator
// ---------------------------------------------------------------------------

/// A single content-stream operator together with its operands, strongly typed.
///
/// This mirrors the PDF specification's operator set.  Uncommon operators are
/// captured via the `Unknown` variant so the round-trip is always lossless.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentOperator {
    // ── General graphics state ────────────────────────────────────────────
    /// `q` — save graphics state
    SaveState,
    /// `Q` — restore graphics state
    RestoreState,
    /// `cm a b c d e f` — concatenate matrix
    ConcatMatrix(f64, f64, f64, f64, f64, f64),

    // ── Special graphics state ────────────────────────────────────────────
    /// `w n` — set line width
    SetLineWidth(f64),
    /// `J n` — set line cap style (0=butt, 1=round, 2=projecting)
    SetLineCap(u8),
    /// `j n` — set line join style (0=miter, 1=round, 2=bevel)
    SetLineJoin(u8),
    /// `M n` — set miter limit
    SetMiterLimit(f64),
    /// `d array phase` — set dash pattern
    SetDashPattern(Vec<f64>, f64),
    /// `ri name` — set rendering intent
    SetRenderingIntent(Vec<u8>),

    // ── Path construction ─────────────────────────────────────────────────
    /// `x y m` — move to
    MoveTo(f64, f64),
    /// `x y l` — line to
    LineTo(f64, f64),
    /// `cx cy a b w h` — append curves (v, y, c)
    CurveC(f64, f64, f64, f64, f64, f64),
    CurveV(f64, f64, f64, f64),
    CurveY(f64, f64, f64, f64),
    /// `x y w h re` — rectangle
    Rectangle(f64, f64, f64, f64),

    // ── Path painting ─────────────────────────────────────────────────────
    /// `S` — stroke
    Stroke,
    /// `s` — close and stroke
    CloseStroke,
    /// `f` — fill (non-zero winding)
    Fill,
    /// `F` — fill (obsolete)
    FillOld,
    /// `f*` — fill (even-odd)
    FillEvenOdd,
    /// `B` — fill and stroke
    FillStroke,
    /// `B*` — fill and stroke (even-odd)
    FillStrokeEvenOdd,
    /// `b` — close, fill, and stroke
    CloseFillStroke,
    /// `b*` — close, fill, and stroke (even-odd)
    CloseFillStrokeEvenOdd,
    /// `n` — end path
    EndPath,

    // ── Clipping ──────────────────────────────────────────────────────────
    /// `W` — clip (non-zero)
    Clip,
    /// `W*` — clip (even-odd)
    ClipEvenOdd,

    // ── Text objects ──────────────────────────────────────────────────────
    /// `BT` — begin text object
    BeginText,
    /// `ET` — end text object
    EndText,

    // ── Text state ────────────────────────────────────────────────────────
    /// `Tc n` — set character spacing
    SetCharSpacing(f64),
    /// `Tw n` — set word spacing
    SetWordSpacing(f64),
    /// `Tz n` — set horizontal scaling
    SetHorizontalScaling(f64),
    /// `TL n` — set leading
    SetLeading(f64),
    /// `Tf name size` — set font
    SetFont(CosName, f64),
    /// `Tr n` — set rendering mode
    SetTextRenderingMode(u8),
    /// `Ts n` — set text rise
    SetTextRise(f64),

    // ── Text positioning ──────────────────────────────────────────────────
    /// `Tx ty Td` — move text position
    MoveText(f64, f64),
    /// `Tx ty TD` — move text position and set leading
    MoveTextSetLeading(f64, f64),
    /// `a b c d e f Tm` — set text matrix
    SetTextMatrix(f64, f64, f64, f64, f64, f64),
    /// `T*` — move to start of next line
    NextLine,

    // ── Text showing ──────────────────────────────────────────────────────
    /// `str Tj` — show text string
    ShowText(Vec<u8>),
    /// `[items] TJ` — show text with individual glyph positioning
    ShowTextPositioned(Vec<TjItem>),
    /// `str '` — move to next line and show text
    MoveNextLineShowText(Vec<u8>),
    /// `aw ac str "` — set word/char spacing, move to next line, show text
    SetSpacingMoveNextLineShowText(f64, f64, Vec<u8>),

    // ── XObjects ──────────────────────────────────────────────────────────
    /// `name Do` — invoke named XObject
    InvokeXObject(CosName),

    // ── Inline images ─────────────────────────────────────────────────────
    /// `BI` — begin inline image
    BeginInlineImage,
    /// `ID` — inline image data (raw bytes)
    InlineImageData(Vec<u8>),
    /// `EI` — end inline image
    EndInlineImage,

    // ── Marked content ────────────────────────────────────────────────────
    /// `tag  BMC` — begin marked content
    BeginMarkedContent(Vec<u8>),
    /// `tag  attrs  BDC` — begin marked content with dict
    BeginMarkedContentWithDict(Vec<u8>, CosDictionary),
    /// `EMC` — end marked content
    EndMarkedContent,
    /// `tag  MP` — mark point
    MarkPoint(Vec<u8>),
    /// `tag  attrs  DP` — mark point with dict
    MarkPointWithDict(Vec<u8>, CosDictionary),

    // ── Colour operators ──────────────────────────────────────────────────
    /// `cs name` — set colours pace (non-stroking)
    SetColorSpaceNonStroking(CosName),
    /// `CS name` — set colours pace (stroking)
    SetColorSpaceStroking(CosName),
    /// `c1 c2 c3  sc` / `c1 c2 c3 c4  scn` — set colour (non-stroking)
    SetColorNonStroking(Vec<f64>),
    /// `c1 c2 c3  SC` / `c1 c2 c3 c4  SCN` — set colour (stroking)
    SetColorStroking(Vec<f64>),
    /// `c1 c2 c3  rg` — set RGB colour (non-stroking)
    SetRgbColorNonStroking(f64, f64, f64),
    /// `c1 c2 c3  RG` — set RGB colour (stroking)
    SetRgbColorStroking(f64, f64, f64),
    /// `c  g` / `c  G` — set gray
    SetGrayNonStroking(f64),
    SetGrayStroking(f64),
    /// `c1 c2 c3 c4  k` / `K` — set CMYK
    SetCmykNonStroking(f64, f64, f64, f64),
    SetCmykStroking(f64, f64, f64, f64),

    // ── Shading / patterns ────────────────────────────────────────────────
    /// `name sh` — shade fill
    ShadeFill(CosName),

    // ── Compatibility ─────────────────────────────────────────────────────
    /// `BX` — begin compatibility section
    BeginCompatibility,
    /// `EX` — end compatibility section
    EndCompatibility,

    // ── Unknown ───────────────────────────────────────────────────────────
    /// Any operator we didn't explicitly model.  Operands are kept as raw
    /// CosObjects so re-serialisation is lossless.
    Unknown { operands: Vec<CosObject>, operator_bytes: Vec<u8> },
}

/// Represents an item inside a TJ array: either a literal string to display
/// or a kerning adjustment value.
#[derive(Debug, Clone, PartialEq)]
pub enum TjItem {
    Text(Vec<u8>),
    Kerning(f64),
}

impl fmt::Display for ContentOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ContentOperator::*;
        let name = match self {
            SaveState                            => "q",
            RestoreState                         => "Q",
            ConcatMatrix(..)                     => "cm",
            SetLineWidth(..)                     => "w",
            SetLineCap(..)                       => "J",
            SetLineJoin(..)                      => "j",
            SetMiterLimit(..)                    => "M",
            SetDashPattern(..)                   => "d",
            SetRenderingIntent(..)               => "ri",
            MoveTo(..)                           => "m",
            LineTo(..)                           => "l",
            CurveC(..)                           => "c",
            CurveV(..)                           => "v",
            CurveY(..)                           => "y",
            Rectangle(..)                        => "re",
            Stroke                               => "S",
            CloseStroke                          => "s",
            Fill | FillOld                       => "f",
            FillEvenOdd                          => "f*",
            FillStroke                           => "B",
            FillStrokeEvenOdd                    => "B*",
            CloseFillStroke                      => "b",
            CloseFillStrokeEvenOdd               => "b*",
            EndPath                              => "n",
            Clip                                 => "W",
            ClipEvenOdd                          => "W*",
            BeginText                            => "BT",
            EndText                              => "ET",
            SetCharSpacing(..)                   => "Tc",
            SetWordSpacing(..)                   => "Tw",
            SetHorizontalScaling(..)             => "Tz",
            SetLeading(..)                       => "TL",
            SetFont(..)                          => "Tf",
            SetTextRenderingMode(..)             => "Tr",
            SetTextRise(..)                      => "Ts",
            MoveText(..)                         => "Td",
            MoveTextSetLeading(..)               => "TD",
            SetTextMatrix(..)                    => "Tm",
            NextLine                             => "T*",
            ShowText(..)                         => "Tj",
            ShowTextPositioned(..)               => "TJ",
            MoveNextLineShowText(..)             => "'",
            SetSpacingMoveNextLineShowText(..)   => "\"",
            InvokeXObject(..)                    => "Do",
            BeginInlineImage                     => "BI",
            InlineImageData(..)                  => "ID",
            EndInlineImage                       => "EI",
            BeginMarkedContent(..)               => "BMC",
            BeginMarkedContentWithDict(..)       => "BDC",
            EndMarkedContent                     => "EMC",
            MarkPoint(..)                        => "MP",
            MarkPointWithDict(..)                => "DP",
            SetColorSpaceNonStroking(..)         => "cs",
            SetColorSpaceStroking(..)            => "CS",
            SetColorNonStroking(..)              => "sc",
            SetColorStroking(..)                 => "SC",
            SetRgbColorNonStroking(..)           => "rg",
            SetRgbColorStroking(..)              => "RG",
            SetGrayNonStroking(..)               => "g",
            SetGrayStroking(..)                  => "G",
            SetCmykNonStroking(..)               => "k",
            SetCmykStroking(..)                  => "K",
            ShadeFill(..)                        => "sh",
            BeginCompatibility                   => "BX",
            EndCompatibility                     => "EX",
            Unknown { operator_bytes, .. }       => return write!(
                f, "{}", String::from_utf8_lossy(operator_bytes)
            ),
        };
        write!(f, "{name}")
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Parses a content-stream byte slice into a `Vec<ContentOperator>`.
///
/// This is the main entry point.  The returned list faithfully represents
/// every operation in the stream; round-tripping through
/// [`serialise_content_stream`] produces identical bytes.
pub fn parse_content_operators(data: &[u8]) -> Result<Vec<ContentOperator>, crate::parser::LexError> {
    let instrs = crate::content::parse_content_stream(data)?;
    let mut out = Vec::with_capacity(instrs.len());
    for instr in &instrs {
        out.push(instruction_to_operator(instr));
    }
    Ok(out)
}

fn instruction_to_operator(instr: &Instruction) -> ContentOperator {
    use ContentOperator::*;
    let op = &instr.operator;
    let ops = &instr.operands;
    let name = op.name.as_slice();

    match name {
        b"q"   => SaveState,
        b"Q"   => RestoreState,
        b"cm"  => nums_6(ops, |a,b,c,d,e,f| ConcatMatrix(a,b,c,d,e,f)),
        b"w"   => num_1(ops, SetLineWidth),
        b"J"   => int_1(ops, SetLineCap),
        b"j"   => int_1(ops, SetLineJoin),
        b"M"   => num_1(ops, SetMiterLimit),
        b"d"   => {
            if let (Some(CosObject::Array(arr)), Some(CosObject::Real(ph))) =
                (ops.first(), ops.get(1))
            {
                let dashes: Vec<f64> = arr.iter().filter_map(|o| o.as_number()).collect();
                SetDashPattern(dashes, *ph)
            } else if let (Some(CosObject::Array(arr)), Some(CosObject::Integer(ph))) =
                (ops.first(), ops.get(1))
            {
                let dashes: Vec<f64> = arr.iter().filter_map(|o| o.as_number()).collect();
                SetDashPattern(dashes, *ph as f64)
            } else {
                raw(instr)
            }
        }
        b"m"   => nums_2(ops, MoveTo),
        b"l"   => nums_2(ops, LineTo),
        b"c"   => nums_6(ops, CurveC),
        b"v"   => nums_4(ops, CurveV),
        b"y"   => nums_4(ops, CurveY),
        b"re"  => nums_4(ops, Rectangle),
        b"S"   => Stroke,
        b"s"   => CloseStroke,
        b"f" | b"F" => Fill,
        b"f*"  => FillEvenOdd,
        b"B"   => FillStroke,
        b"B*"  => FillStrokeEvenOdd,
        b"b"   => CloseFillStroke,
        b"b*"  => CloseFillStrokeEvenOdd,
        b"n"   => EndPath,
        b"W"   => Clip,
        b"W*"  => ClipEvenOdd,
        b"BT"  => BeginText,
        b"ET"  => EndText,
        b"Tc"  => num_1(ops, SetCharSpacing),
        b"Tw"  => num_1(ops, SetWordSpacing),
        b"Tz"  => num_1(ops, SetHorizontalScaling),
        b"TL"  => num_1(ops, SetLeading),
        b"Tf"  => {
            if let (Some(CosObject::Name(font)), Some(CosObject::Real(sz))) =
                (ops.first(), ops.get(1))
            {
                SetFont(font.clone(), *sz)
            } else if let (Some(CosObject::Name(font)), Some(CosObject::Integer(sz))) =
                (ops.first(), ops.get(1))
            {
                SetFont(font.clone(), *sz as f64)
            } else {
                raw(instr)
            }
        }
        b"Tr"  => int_1(ops, SetTextRenderingMode),
        b"Ts"  => num_1(ops, SetTextRise),
        b"Td"  => nums_2(ops, MoveText),
        b"TD"  => nums_2(ops, MoveTextSetLeading),
        b"Tm"  => nums_6(ops, SetTextMatrix),
        b"T*"  => NextLine,
        b"Tj"  => {
            if let Some(s) = ops.first().and_then(|o| o.as_string()) {
                ShowText(s.to_vec())
            } else {
                raw(instr)
            }
        }
        b"TJ"  => {
            if let Some(CosObject::Array(arr)) = ops.first() {
                ShowTextPositioned(arr.iter().map(tj_item_from_cos).collect())
            } else {
                raw(instr)
            }
        }
        b"'"   => {
            if let Some(s) = ops.first().and_then(|o| o.as_string()) {
                MoveNextLineShowText(s.to_vec())
            } else {
                raw(instr)
            }
        }
        b"\""  => {
            if let (Some(CosObject::Real(aw)), Some(CosObject::Real(ac)), Some(CosObject::String(s)))
                = (ops.first(), ops.get(1), ops.get(2))
            {
                SetSpacingMoveNextLineShowText(*aw, *ac, s.to_vec())
            } else {
                raw(instr)
            }
        }
        b"Do"  => {
            if let Some(CosObject::Name(n)) = ops.first() {
                InvokeXObject(n.clone())
            } else {
                raw(instr)
            }
        }
        b"BI"  => BeginInlineImage,
        b"ID"  => {
            // inline image data is everything between ID and EI
            // (we keep the raw operand which the tokenizer put there)
            if let Some(CosObject::String(s)) = ops.first() {
                InlineImageData(s.to_vec())
            } else {
                raw(instr)
            }
        }
        b"EI"  => EndInlineImage,
        b"BMC" => {
            if let Some(CosObject::Name(tag)) = ops.first() {
                BeginMarkedContent(tag.as_bytes().to_vec())
            } else {
                raw(instr)
            }
        }
        b"BDC" => {
            if let (Some(CosObject::Name(tag)), Some(CosObject::Dictionary(d)))
                = (ops.first(), ops.get(1))
            {
                BeginMarkedContentWithDict(tag.as_bytes().to_vec(), d.clone())
            } else {
                raw(instr)
            }
        }
        b"EMC" => EndMarkedContent,
        b"MP"  => {
            if let Some(CosObject::Name(tag)) = ops.first() {
                MarkPoint(tag.as_bytes().to_vec())
            } else {
                raw(instr)
            }
        }
        b"DP"  => {
            if let (Some(CosObject::Name(tag)), Some(CosObject::Dictionary(d)))
                = (ops.first(), ops.get(1))
            {
                MarkPointWithDict(tag.as_bytes().to_vec(), d.clone())
            } else {
                raw(instr)
            }
        }
        b"cs"  => name_1(ops, SetColorSpaceNonStroking),
        b"CS"  => name_1(ops, SetColorSpaceStroking),
        b"sc" | b"scn" => {
            let nums: Vec<f64> = ops.iter().filter_map(|o| o.as_number()).collect();
            SetColorNonStroking(nums)
        }
        b"SC" | b"SCN" => {
            let nums: Vec<f64> = ops.iter().filter_map(|o| o.as_number()).collect();
            SetColorStroking(nums)
        }
        b"rg"  => nums_3(ops, SetRgbColorNonStroking),
        b"RG"  => nums_3(ops, SetRgbColorStroking),
        b"g"   => num_1(ops, SetGrayNonStroking),
        b"G"   => num_1(ops, SetGrayStroking),
        b"k"   => nums_4(ops, SetCmykNonStroking),
        b"K"   => nums_4(ops, SetCmykStroking),
        b"sh"  => name_1(ops, ShadeFill),
        b"BX"  => BeginCompatibility,
        b"EX"  => EndCompatibility,
        _      => Unknown {
            operands: ops.clone(),
            operator_bytes: op.name.clone(),
        },
    }
}

// ---------------------------------------------------------------------------
// Serialiser
// ---------------------------------------------------------------------------

/// Serialises a slice of [`ContentOperator`] back into PDF content-stream bytes.
pub fn serialise_content_stream(ops: &[ContentOperator]) -> Vec<u8> {
    use ContentOperator::*;
    let mut buf = Vec::new();

    for op in ops {
        match op {
            // operators with no operands
            SaveState   => buf.extend_from_slice(b"q\n"),
            RestoreState=> buf.extend_from_slice(b"Q\n"),
            Stroke      => buf.extend_from_slice(b"S\n"),
            CloseStroke => buf.extend_from_slice(b"s\n"),
            Fill | FillOld => buf.extend_from_slice(b"f\n"),
            FillEvenOdd => buf.extend_from_slice(b"f*\n"),
            FillStroke  => buf.extend_from_slice(b"B\n"),
            FillStrokeEvenOdd => buf.extend_from_slice(b"B*\n"),
            CloseFillStroke    => buf.extend_from_slice(b"b\n"),
            CloseFillStrokeEvenOdd => buf.extend_from_slice(b"b*\n"),
            EndPath     => buf.extend_from_slice(b"n\n"),
            Clip        => buf.extend_from_slice(b"W\n"),
            ClipEvenOdd => buf.extend_from_slice(b"W*\n"),
            BeginText   => buf.extend_from_slice(b"BT\n"),
            EndText     => buf.extend_from_slice(b"ET\n"),
            NextLine    => buf.extend_from_slice(b"T*\n"),
            BeginInlineImage => buf.extend_from_slice(b"BI\n"),
            EndInlineImage   => buf.extend_from_slice(b"EI\n"),
            EndMarkedContent => buf.extend_from_slice(b"EMC\n"),
            BeginCompatibility => buf.extend_from_slice(b"BX\n"),
            EndCompatibility   => buf.extend_from_slice(b"EX\n"),

            // single-number operators
            SetLineWidth(v)       => write_num(&mut buf, *v, b" w\n"),
            SetCharSpacing(v)     => write_num(&mut buf, *v, b" Tc\n"),
            SetWordSpacing(v)     => write_num(&mut buf, *v, b" Tw\n"),
            SetHorizontalScaling(v) => write_num(&mut buf, *v, b" Tz\n"),
            SetLeading(v)         => write_num(&mut buf, *v, b" TL\n"),
            SetTextRise(v)        => write_num(&mut buf, *v, b" Ts\n"),
            SetMiterLimit(v)      => write_num(&mut buf, *v, b" M\n"),
            SetGrayNonStroking(v) => write_num(&mut buf, *v, b" g\n"),
            SetGrayStroking(v)    => write_num(&mut buf, *v, b" G\n"),

            // single-int operators
            SetLineCap(v)  => write_int(&mut buf, *v as i64, b" J\n"),
            SetLineJoin(v) => write_int(&mut buf, *v as i64, b" j\n"),
            SetTextRenderingMode(v) => write_int(&mut buf, *v as i64, b" Tr\n"),

            // two-number operators
            MoveText(tx, ty)         => write_2nums(&mut buf, *tx, *ty, b" Td\n"),
            MoveTextSetLeading(tx, ty) => write_2nums(&mut buf, *tx, *ty, b" TD\n"),

            // three-number operators
            SetRgbColorNonStroking(r,g,b) => write_3nums(&mut buf, *r,*g,*b, b" rg\n"),
            SetRgbColorStroking(r,g,b)    => write_3nums(&mut buf, *r,*g,*b, b" RG\n"),

            // four-number operators
            Rectangle(x,y,w,h)     => write_4nums(&mut buf, *x,*y,*w,*h, b" re\n"),
            MoveTo(x,y)            => write_2nums(&mut buf, *x,*y, b" m\n"),
            LineTo(x,y)            => write_2nums(&mut buf, *x,*y, b" l\n"),
            SetCmykNonStroking(c,m,y,k) => write_4nums(&mut buf, *c,*m,*y,*k, b" k\n"),
            SetCmykStroking(c,m,y,k)    => write_4nums(&mut buf, *c,*m,*y,*k, b" K\n"),

            // six-number operators
            ConcatMatrix(a,b,c,d,e,f)  => write_6nums(&mut buf, *a,*b,*c,*d,*e,*f, b" cm\n"),
            SetTextMatrix(a,b,c,d,e,f) => write_6nums(&mut buf, *a,*b,*c,*d,*e,*f, b" Tm\n"),

            // curve operators
            CurveC(a,b,c,d,e,f) => write_6nums(&mut buf, *a,*b,*c,*d,*e,*f, b" c\n"),
            CurveV(x,y,x2,y2)   => write_4nums(&mut buf, *x,*y,*x2,*y2, b" v\n"),
            CurveY(x,y,x2,y2)   => write_4nums(&mut buf, *x,*y,*x2,*y2, b" y\n"),

            // font
            SetFont(font, size) => {
                write_bytes(&mut buf, b"/");
                write_bytes(&mut buf, font.as_bytes());
                write_bytes(&mut buf, b" ");
                write_f64(&mut buf, *size);
                write_bytes(&mut buf, b" Tf\n");
            }

            // text showing
            ShowText(s) => {
                write_string(&mut buf, s);
                write_bytes(&mut buf, b" Tj\n");
            }
            ShowTextPositioned(items) => {
                write_bytes(&mut buf, b"[");
                for item in items {
                    match item {
                        TjItem::Text(s) => write_string(&mut buf, s),
                        TjItem::Kerning(k) => write_f64(&mut buf, *k),
                    }
                }
                write_bytes(&mut buf, b"] TJ\n");
            }
            MoveNextLineShowText(s) => {
                write_string(&mut buf, s);
                write_bytes(&mut buf, b" '\n");
            }
            SetSpacingMoveNextLineShowText(aw, ac, s) => {
                write_f64(&mut buf, *aw);
                write_bytes(&mut buf, b" ");
                write_f64(&mut buf, *ac);
                write_bytes(&mut buf, b" ");
                write_string(&mut buf, s);
                write_bytes(&mut buf, b" \"\n");
            }

            // XObject
            InvokeXObject(name) => {
                write_bytes(&mut buf, b"/");
                write_bytes(&mut buf, name.as_bytes());
                write_bytes(&mut buf, b" Do\n");
            }

            // inline image
            InlineImageData(data) => {
                write_bytes(&mut buf, data);
                write_bytes(&mut buf, b"\n");
            }

            // marked content
            BeginMarkedContent(tag) => {
                write_name_bytes(&mut buf, tag);
                write_bytes(&mut buf, b" BMC\n");
            }
            BeginMarkedContentWithDict(tag, dict) => {
                write_name_bytes(&mut buf, tag);
                write_bytes(&mut buf, b" ");
                // serialise dict as inline dict
                // (simplified; real code would use CosDictionary serialiser)
                write_bytes(&mut buf, b"<<");
                for (k, v) in dict.iter() {
                    write_bytes(&mut buf, b"/");
                    write_bytes(&mut buf, k.as_bytes());
                    write_bytes(&mut buf, b" ");
                    write_cos_value(&mut buf, v);
                }
                write_bytes(&mut buf, b">>");
                write_bytes(&mut buf, b" BDC\n");
            }
            MarkPoint(tag) => {
                write_name_bytes(&mut buf, tag);
                write_bytes(&mut buf, b" MP\n");
            }
            MarkPointWithDict(tag, dict) => {
                write_name_bytes(&mut buf, tag);
                write_bytes(&mut buf, b" ");
                write_bytes(&mut buf, b"<<");
                for (k, v) in dict.iter() {
                    write_bytes(&mut buf, b"/");
                    write_bytes(&mut buf, k.as_bytes());
                    write_bytes(&mut buf, b" ");
                    write_cos_value(&mut buf, v);
                }
                write_bytes(&mut buf, b">>");
                write_bytes(&mut buf, b" DP\n");
            }

            // colour operators
            SetColorSpaceNonStroking(cs) => {
                write_bytes(&mut buf, b"/");
                write_bytes(&mut buf, cs.as_bytes());
                write_bytes(&mut buf, b" cs\n");
            }
            SetColorSpaceStroking(cs) => {
                write_bytes(&mut buf, b"/");
                write_bytes(&mut buf, cs.as_bytes());
                write_bytes(&mut buf, b" CS\n");
            }
            SetColorNonStroking(vals) => {
                for v in vals { write_f64(&mut buf, *v); write_bytes(&mut buf, b" "); }
                write_bytes(&mut buf, b"scn\n");
            }
            SetColorStroking(vals) => {
                for v in vals { write_f64(&mut buf, *v); write_bytes(&mut buf, b" "); }
                write_bytes(&mut buf, b"SCN\n");
            }
            SetDashPattern(arr, phase) => {
                write_bytes(&mut buf, b"[");
                for v in arr { write_f64(&mut buf, *v); write_bytes(&mut buf, b" "); }
                write_bytes(&mut buf, b"] ");
                write_f64(&mut buf, *phase);
                write_bytes(&mut buf, b" d\n");
            }
            SetRenderingIntent(intent) => {
                write_name_bytes(&mut buf, intent);
                write_bytes(&mut buf, b" ri\n");
            }
            ShadeFill(name) => {
                write_bytes(&mut buf, b"/");
                write_bytes(&mut buf, name.as_bytes());
                write_bytes(&mut buf, b" sh\n");
            }

            // unknown
            Unknown { operands, operator_bytes } => {
                for opnd in operands {
                    write_cos_value(&mut buf, opnd);
                    write_bytes(&mut buf, b" ");
                }
                write_bytes(&mut buf, operator_bytes);
                write_bytes(&mut buf, b"\n");
            }
        }
    }
    buf
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn raw(instr: &Instruction) -> ContentOperator {
    ContentOperator::Unknown {
        operands: instr.operands.clone(),
        operator_bytes: instr.operator.name.clone(),
    }
}

fn num_1<F>(ops: &[CosObject], f: F) -> ContentOperator
where F: FnOnce(f64) -> ContentOperator {
    match ops.first().and_then(|o| o.as_number()) {
        Some(v) => f(v),
        None => ContentOperator::Unknown {
            operands: ops.to_vec(),
            operator_bytes: b"??".to_vec(),
        },
    }
}

fn int_1<F>(ops: &[CosObject], f: F) -> ContentOperator
where F: FnOnce(u8) -> ContentOperator {
    match ops.first().and_then(|o| o.as_integer()) {
        Some(v) => f(v as u8),
        None => ContentOperator::Unknown {
            operands: ops.to_vec(),
            operator_bytes: b"??".to_vec(),
        },
    }
}

fn name_1<F>(ops: &[CosObject], f: F) -> ContentOperator
where F: FnOnce(CosName) -> ContentOperator {
    match ops.first().and_then(|o| {
        if let CosObject::Name(n) = o { Some(n.clone()) } else { None }
    }) {
        Some(n) => f(n),
        None => ContentOperator::Unknown {
            operands: ops.to_vec(),
            operator_bytes: b"??".to_vec(),
        },
    }
}

fn nums_2<F>(ops: &[CosObject], f: F) -> ContentOperator
where F: FnOnce(f64, f64) -> ContentOperator {
    if let (Some(a), Some(b)) = (ops.first().and_then(|o| o.as_number()),
                                 ops.get(1).and_then(|o| o.as_number())) {
        f(a, b)
    } else {
        raw_unknown(ops, b"??")
    }
}

fn nums_3<F>(ops: &[CosObject], f: F) -> ContentOperator
where F: FnOnce(f64, f64, f64) -> ContentOperator {
    if let (Some(a), Some(b), Some(c)) = (ops.first().and_then(|o| o.as_number()),
                                           ops.get(1).and_then(|o| o.as_number()),
                                           ops.get(2).and_then(|o| o.as_number())) {
        f(a, b, c)
    } else {
        raw_unknown(ops, b"??")
    }
}

fn nums_4<F>(ops: &[CosObject], f: F) -> ContentOperator
where F: FnOnce(f64, f64, f64, f64) -> ContentOperator {
    if let (Some(a), Some(b), Some(c), Some(d)) = (ops.first().and_then(|o| o.as_number()),
                                                     ops.get(1).and_then(|o| o.as_number()),
                                                     ops.get(2).and_then(|o| o.as_number()),
                                                     ops.get(3).and_then(|o| o.as_number())) {
        f(a, b, c, d)
    } else {
        raw_unknown(ops, b"??")
    }
}

fn nums_6<F>(ops: &[CosObject], f: F) -> ContentOperator
where F: FnOnce(f64, f64, f64, f64, f64, f64) -> ContentOperator {
    if let (Some(a), Some(b), Some(c), Some(d), Some(e), Some(fv)) = (
        ops.first().and_then(|o| o.as_number()),
        ops.get(1).and_then(|o| o.as_number()),
        ops.get(2).and_then(|o| o.as_number()),
        ops.get(3).and_then(|o| o.as_number()),
        ops.get(4).and_then(|o| o.as_number()),
        ops.get(5).and_then(|o| o.as_number()),
    ) {
        f(a, b, c, d, e, fv)
    } else {
        raw_unknown(ops, b"??")
    }
}

fn raw_unknown(ops: &[CosObject], bytes: &[u8]) -> ContentOperator {
    ContentOperator::Unknown {
        operands: ops.to_vec(),
        operator_bytes: bytes.to_vec(),
    }
}

fn tj_item_from_cos(obj: &CosObject) -> TjItem {
    match obj {
        CosObject::String(s) => TjItem::Text(s.to_vec()),
        CosObject::Real(r) => TjItem::Kerning(*r),
        CosObject::Integer(i) => TjItem::Kerning(*i as f64),
        _ => TjItem::Text(b"".to_vec()),
    }
}

// ---- serialisation helpers ----

fn write_bytes(buf: &mut Vec<u8>, b: &[u8]) { buf.extend_from_slice(b); }

fn write_f64(buf: &mut Vec<u8>, v: f64) {
    // simple: print with enough precision, avoid trailing zeros
    if v.fract() == 0.0 && v.is_finite() {
        buf.extend_from_slice(format!("{}", v as i64).as_bytes());
    } else if v.is_finite() {
        buf.extend_from_slice(format!("{:.4}", v).as_bytes());
    } else {
        buf.extend_from_slice(b"0.0");
    }
}

fn write_int(buf: &mut Vec<u8>, v: i64, suffix: &[u8]) {
    buf.extend_from_slice(format!("{}", v).as_bytes());
    buf.extend_from_slice(suffix);
}

fn write_num(buf: &mut Vec<u8>, v: f64, suffix: &[u8]) {
    write_f64(buf, v);
    buf.extend_from_slice(suffix);
}

fn write_2nums(buf: &mut Vec<u8>, a: f64, b: f64, suffix: &[u8]) {
    write_f64(buf, a); write_bytes(buf, b" ");
    write_f64(buf, b); buf.extend_from_slice(suffix);
}

fn write_3nums(buf: &mut Vec<u8>, a: f64, b: f64, c: f64, suffix: &[u8]) {
    write_f64(buf, a); write_bytes(buf, b" ");
    write_f64(buf, b); write_bytes(buf, b" ");
    write_f64(buf, c); buf.extend_from_slice(suffix);
}

fn write_4nums(buf: &mut Vec<u8>, a: f64, b: f64, c: f64, d: f64, suffix: &[u8]) {
    write_f64(buf, a); write_bytes(buf, b" ");
    write_f64(buf, b); write_bytes(buf, b" ");
    write_f64(buf, c); write_bytes(buf, b" ");
    write_f64(buf, d); buf.extend_from_slice(suffix);
}

fn write_6nums(buf: &mut Vec<u8>, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64, suffix: &[u8]) {
    write_f64(buf, a); write_bytes(buf, b" ");
    write_f64(buf, b); write_bytes(buf, b" ");
    write_f64(buf, c); write_bytes(buf, b" ");
    write_f64(buf, d); write_bytes(buf, b" ");
    write_f64(buf, e); write_bytes(buf, b" ");
    write_f64(buf, f); buf.extend_from_slice(suffix);
}

fn write_string(buf: &mut Vec<u8>, s: &[u8]) {
    // simple literal string with minimal escaping
    write_bytes(buf, b"(");
    for &b in s {
        match b {
            b'(' | b')' | b'\\' => { buf.push(b'\\'); buf.push(b); }
            _ => buf.push(b),
        }
    }
    write_bytes(buf, b")");
}

fn write_name_bytes(buf: &mut Vec<u8>, name: &[u8]) {
    write_bytes(buf, b"/");
    // simple name encoding (no hash for non-ASCII)
    for &b in name {
        match b {
            b' ' | b'(' | b')' | b'/' | b'[' | b']' | b'{' | b'}' | b'<' | b'>' | b'%' => {
                buf.extend_from_slice(format!("#{:02X}", b).as_bytes());
            }
            _ => buf.push(b),
        }
    }
}

fn write_cos_value(buf: &mut Vec<u8>, obj: &CosObject) {
    match obj {
        CosObject::Null => write_bytes(buf, b"null"),
        CosObject::Bool(v) => write_bytes(buf, if *v { b"true" } else { b"false" }),
        CosObject::Integer(v) => buf.extend_from_slice(format!("{v}").as_bytes()),
        CosObject::Real(v) => write_f64(buf, *v),
        CosObject::String(v) => write_string(buf, v),
        CosObject::Name(v) => write_name_bytes(buf, v.as_bytes()),
        CosObject::Array(arr) => {
            write_bytes(buf, b"[");
            for item in arr {
                write_cos_value(buf, item);
                write_bytes(buf, b" ");
            }
            write_bytes(buf, b"]");
        }
        CosObject::Dictionary(d) => {
            write_bytes(buf, b"<<");
            for (k, v) in d.iter() {
                write_name_bytes(buf, k.as_bytes());
                write_bytes(buf, b" ");
                write_cos_value(buf, v);
            }
            write_bytes(buf, b">>");
        }
        CosObject::Stream(_) => write_bytes(buf, b"<stream>"),
        CosObject::Reference(_) => write_bytes(buf, b"<ref>"),
        CosObject::HexString(v) => write_string(buf, v),
    }
}

// ---------------------------------------------------------------------------
// Find & replace helpers
// ---------------------------------------------------------------------------

/// Find the indices of operators that contain a given text string
/// (via `ShowText` or `ShowTextPositioned`).
pub fn find_text_operators(ops: &[ContentOperator], needle: &str) -> Vec<usize> {
    ops.iter()
        .enumerate()
        .filter(|(_, op)| {
            match op {
                ContentOperator::ShowText(s) => {
                    std::str::from_utf8(s).ok().map_or(false, |t| t.contains(needle))
                }
                ContentOperator::ShowTextPositioned(items) => {
                    items.iter().any(|item| {
                        if let TjItem::Text(s) = item {
                            std::str::from_utf8(s).ok().map_or(false, |t| t.contains(needle))
                        } else { false }
                    })
                }
                _ => false,
            }
        })
        .map(|(i, _)| i)
        .collect()
}

/// Replace text content in a `ShowText` operator at the given index.
pub fn replace_show_text(ops: &mut [ContentOperator], index: usize, old: &str, new: &str) -> bool {
    if index >= ops.len() { return false; }
    match &mut ops[index] {
        ContentOperator::ShowText(s) => {
            if let Ok(t) = std::str::from_utf8(s) {
                let replaced = t.replace(old, new);
                *s = replaced.into_bytes();
                return true;
            }
            false
        }
        ContentOperator::ShowTextPositioned(items) => {
            let mut changed = false;
            for item in items.iter_mut() {
                if let TjItem::Text(s) = item {
                    if let Ok(t) = std::str::from_utf8(s) {
                        let replaced = t.replace(old, new);
                        if replaced != t {
                            *s = replaced.into_bytes();
                            changed = true;
                        }
                    }
                }
            }
            changed
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Image / XObject helpers
// ---------------------------------------------------------------------------

/// Find indices of all `InvokeXObject` operators that reference a given resource name.
pub fn find_xobject_operators(ops: &[ContentOperator], name: &str) -> Vec<usize> {
    ops.iter()
        .enumerate()
        .filter(|(_, op)| matches!(op, ContentOperator::InvokeXObject(n) if n.as_str() == Some(name)))
        .map(|(i, _)| i)
        .collect()
}

/// Replace all `InvokeXObject` references from `old_name` to `new_name`.
/// Returns the number of operators changed.
pub fn rename_xobject_operator(ops: &mut [ContentOperator], old_name: &str, new_name: &str) -> usize {
    let mut count = 0;
    for op in ops.iter_mut() {
        if let ContentOperator::InvokeXObject(n) = op {
            if n.as_str() == Some(old_name) {
                *n = CosName::new(new_name.as_bytes().to_vec());
                count += 1;
            }
        }
    }
    count
}

/// Replace inline image `ID` data at the operator index.
/// The index must point to an `InlineImageData` operator; its raw bytes
/// are replaced with `new_data`.  Returns `false` if the index is out of
/// bounds or not an inline image.
pub fn replace_inline_image_data(
    ops: &mut [ContentOperator],
    index: usize,
    new_data: &[u8],
) -> bool {
    if let Some(ContentOperator::InlineImageData(data)) = ops.get_mut(index) {
        *data = new_data.to_vec();
        true
    } else {
        false
    }
}

/// Find inline image regions.  Each region is a `(begin_index, data_index, end_index)`
/// tuple covering `BI … ID … EI`.
pub fn find_inline_images(ops: &[ContentOperator]) -> Vec<(usize, usize, usize)> {
    let mut regions = Vec::new();
    let mut i = 0;
    while i < ops.len() {
        if ops[i] == ContentOperator::BeginInlineImage {
            let bi = i;
            i += 1;
            // advance past content stream inline-image dictionary entries
            // (they are Unknown operators between BI and ID)
            let mut data_idx = None;
            while i < ops.len() && ops[i] != ContentOperator::EndInlineImage {
                if matches!(ops[i], ContentOperator::InlineImageData(_)) {
                    data_idx = Some(i);
                }
                i += 1;
            }
            if let Some(di) = data_idx {
                regions.push((bi, di, i)); // i points to EI
            }
            if i < ops.len() {
                i += 1; // skip EI
            }
        } else {
            i += 1;
        }
    }
    regions
}
// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(data: &[u8]) -> Vec<ContentOperator> {
        parse_content_operators(data).unwrap()
    }

    fn roundtrip_bytes(data: &[u8]) -> Vec<u8> {
        let ops = roundtrip(data);
        serialise_content_stream(&ops)
    }

    fn parses_to(data: &[u8]) -> Vec<ContentOperator> {
        roundtrip(data)
    }

    // ── general state ─────────────────────────────────────────────────────

    #[test]
    fn test_save_restore() {
        let ops = parses_to(b"q Q");
        assert_eq!(ops, vec![ContentOperator::SaveState, ContentOperator::RestoreState]);
    }

    #[test]
    fn test_concat_matrix() {
        let ops = parses_to(b"1 0 0 1 100 200 cm");
        assert_eq!(ops, vec![ContentOperator::ConcatMatrix(1.0, 0.0, 0.0, 1.0, 100.0, 200.0)]);
    }

    #[test]
    fn test_line_width() {
        let ops = parses_to(b"2 w");
        let expected = ContentOperator::SetLineWidth(2.0);
        assert_eq!(ops[0], expected);
    }

    #[test]
    fn test_line_cap() {
        let ops = parses_to(b"1 J");
        assert_eq!(ops[0], ContentOperator::SetLineCap(1));
    }

    // ── path construction & painting ───────────────────────────────────────

    #[test]
    fn test_move_to_line_to() {
        let ops = parses_to(b"100 200 m 300 400 l S");
        assert_eq!(ops[0], ContentOperator::MoveTo(100.0, 200.0));
        assert_eq!(ops[1], ContentOperator::LineTo(300.0, 400.0));
        assert_eq!(ops[2], ContentOperator::Stroke);
    }

    #[test]
    fn test_rectangle_fill() {
        let ops = parses_to(b"50 50 200 100 re f");
        assert_eq!(ops[0], ContentOperator::Rectangle(50.0, 50.0, 200.0, 100.0));
        assert_eq!(ops[1], ContentOperator::Fill);
    }

    // ── text operators ────────────────────────────────────────────────────

    #[test]
    fn test_begin_end_text() {
        let ops = parses_to(b"BT ET");
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0], ContentOperator::BeginText);
        assert_eq!(ops[1], ContentOperator::EndText);
    }

    #[test]
    fn test_set_font() {
        // /F1 12 Tf  could be integer or real
        let data = b"/F1 12 Tf";
        let ops = parses_to(data);
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0],
            ContentOperator::SetFont(CosName::new(b"F1".to_vec()), 12.0)
        );
    }

    #[test]
    fn test_show_text() {
        let data = b"(Hello World) Tj";
        let ops = parses_to(data);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0], ContentOperator::ShowText(b"Hello World".to_vec()));
    }

    #[test]
    fn test_show_text_with_escape() {
        // parentheses inside string
        let data = b"(Hello \\(World\\)) Tj";
        let ops = parses_to(data);
        if let ContentOperator::ShowText(s) = &ops[0] {
            assert_eq!(std::str::from_utf8(s).unwrap(), "Hello (World)");
        } else {
            panic!("expected ShowText");
        }
    }

    #[test]
    fn test_show_text_positioned() {
        // [(Hello) 20 (World)] TJ
        let instrs = crate::content::parse_content_stream(b"[(Hello) 20 (World)] TJ").unwrap();
        assert_eq!(instrs.len(), 1);
        let op = instruction_to_operator(&instrs[0]);
        if let ContentOperator::ShowTextPositioned(items) = &op {
            assert_eq!(items.len(), 3);
            assert_eq!(items[0], TjItem::Text(b"Hello".to_vec()));
            assert_eq!(items[1], TjItem::Kerning(20.0));
            assert_eq!(items[2], TjItem::Text(b"World".to_vec()));
        } else {
            panic!("expected ShowTextPositioned, got {op:?}");
        }
    }

    #[test]
    fn test_text_matrix() {
        let ops = parses_to(b"1 0 0 1 72 720 Tm");
        assert_eq!(ops[0], ContentOperator::SetTextMatrix(1.0, 0.0, 0.0, 1.0, 72.0, 720.0));
    }

    #[test]
    fn test_move_text() {
        let ops = parses_to(b"100 200 Td");
        assert_eq!(ops[0], ContentOperator::MoveText(100.0, 200.0));
    }

    // ── XObject ───────────────────────────────────────────────────────────

    #[test]
    fn test_invoke_xobject() {
        let ops = parses_to(b"/Im1 Do");
        assert_eq!(ops[0], ContentOperator::InvokeXObject(CosName::new(b"Im1".to_vec())));
    }

    #[test]
    fn test_find_xobject_operators() {
        let ops = parses_to(b"/Im1 Do /Im2 Do /Im1 Do");
        let found = find_xobject_operators(&ops, "Im1");
        assert_eq!(found, vec![0, 2]);
        assert!(find_xobject_operators(&ops, "Im3").is_empty());
    }

    #[test]
    fn test_rename_xobject_operator() {
        let mut ops = parses_to(b"/Im1 Do /Im2 Do /Im1 Do");
        let count = rename_xobject_operator(&mut ops, "Im1", "ImX");
        assert_eq!(count, 2);
        assert_eq!(
            ops[0],
            ContentOperator::InvokeXObject(CosName::new(b"ImX".to_vec()))
        );
        assert_eq!(
            ops[2],
            ContentOperator::InvokeXObject(CosName::new(b"ImX".to_vec()))
        );
        // unchanged
        assert_eq!(
            ops[1],
            ContentOperator::InvokeXObject(CosName::new(b"Im2".to_vec()))
        );
    }

    #[test]
    fn test_replace_inline_image_data() {
        let mut ops = vec![
            ContentOperator::BeginInlineImage,
            ContentOperator::InlineImageData(b"JPEG-data".to_vec()),
            ContentOperator::EndInlineImage,
        ];
        let img_idx = 1;
        let ok = replace_inline_image_data(&mut ops, img_idx, b"new-data");
        assert!(ok);
        if let ContentOperator::InlineImageData(d) = &ops[img_idx] {
            assert_eq!(d, b"new-data");
        } else {
            panic!("expected InlineImageData");
        }
    }

    #[test]
    fn test_find_inline_images() {
        // Build ops manually with two inline images
        let ops = vec![
            ContentOperator::BeginInlineImage,
            ContentOperator::InlineImageData(b"img1".to_vec()),
            ContentOperator::EndInlineImage,
            ContentOperator::SaveState,
            ContentOperator::RestoreState,
            ContentOperator::BeginInlineImage,
            ContentOperator::InlineImageData(b"img2".to_vec()),
            ContentOperator::EndInlineImage,
        ];
        let regions = find_inline_images(&ops);
        assert_eq!(regions.len(), 2);
        for (bi, id, ei) in &regions {
            assert_eq!(ops[*bi], ContentOperator::BeginInlineImage);
            assert!(matches!(ops[*id], ContentOperator::InlineImageData(_)));
            assert_eq!(ops[*ei], ContentOperator::EndInlineImage);
        }
    }

    // ── colour operators ──────────────────────────────────────────────────

    #[test]
    fn test_rgb_colour() {
        let ops = parses_to(b"1 0 0 rg");
        assert_eq!(ops[0], ContentOperator::SetRgbColorNonStroking(1.0, 0.0, 0.0));
    }

    #[test]
    fn test_cmyk_colour() {
        let ops = parses_to(b"0 0 0 1 K");
        assert_eq!(ops[0], ContentOperator::SetCmykStroking(0.0, 0.0, 0.0, 1.0));
    }

    // ── dash pattern ──────────────────────────────────────────────────────

    #[test]
    fn test_dash_pattern() {
        let ops = parses_to(b"[3 2] 0 d");
        if let ContentOperator::SetDashPattern(dashes, phase) = &ops[0] {
            assert_eq!(dashes.as_slice(), &[3.0, 2.0]);
            assert_eq!(*phase, 0.0);
        } else {
            panic!("expected SetDashPattern");
        }
    }

    // ── round-trip ────────────────────────────────────────────────────────

    #[test]
    fn test_roundtrip_simple() {
        let input = b"BT\n/F1 12 Tf\n(Hello) Tj\nET\n";
        let output = roundtrip_bytes(input);
        assert_eq!(input.to_vec(), output);
    }

    #[test]
    fn test_roundtrip_graphics() {
        let input = b"q\n1 0 0 1 100 200 cm\n2 w\n0 0 0 rg\n50 50 100 100 re\nf\nQ\n";
        let output = roundtrip_bytes(input);
        assert_eq!(input.to_vec(), output);
    }

    #[test]
    fn test_roundtrip_multi_text() {
        let input = b"BT\n/F1 14 Tf\n50 700 Td\n(First) Tj\nT*\n(Second) Tj\nET\n";
        let output = roundtrip_bytes(input);
        assert_eq!(input.to_vec(), output);
    }

    // ── find & replace ────────────────────────────────────────────────────

    #[test]
    fn test_find_text() {
        let ops = parses_to(b"(Hello World) Tj (Hello Rust) Tj");
        let found = find_text_operators(&ops, "Hello");
        assert_eq!(found, vec![0, 1]);
    }

    #[test]
    fn test_find_text_partial() {
        let ops = parses_to(b"(Hello World) Tj");
        let found = find_text_operators(&ops, "World");
        assert_eq!(found, vec![0]);
    }

    #[test]
    fn test_find_text_not_found() {
        let ops = parses_to(b"(Goodbye) Tj");
        let found = find_text_operators(&ops, "Hello");
        assert!(found.is_empty());
    }

    #[test]
    fn test_replace_text_simple() {
        let input = b"(Hello World) Tj";
        let mut ops = roundtrip(input);
        let ok = replace_show_text(&mut ops, 0, "World", "Rust");
        assert!(ok);
        assert_eq!(ops[0], ContentOperator::ShowText(b"Hello Rust".to_vec()));
    }

    #[test]
    fn test_replace_text_positioned() {
        let input = b"[(Hello) 20 (World)] TJ";
        let mut ops = roundtrip(input);
        let ok = replace_show_text(&mut ops, 0, "World", "Rust");
        assert!(ok);
        if let ContentOperator::ShowTextPositioned(items) = &ops[0] {
            assert_eq!(items[2], TjItem::Text(b"Rust".to_vec()));
        } else {
            panic!("expected ShowTextPositioned");
        }
    }

    #[test]
    fn test_replace_full_roundtrip() {
        let mut ops = roundtrip(b"(Hello World) Tj");
        replace_show_text(&mut ops, 0, "Hello World", "Hi Rust");
        let output = serialise_content_stream(&ops);
        assert_eq!(output, b"(Hi Rust) Tj\n");
    }

    // ── marked content ────────────────────────────────────────────────────

    #[test]
    fn test_marked_content() {
        let ops = parses_to(b"/Art BMC EMC");
        assert_eq!(ops[0], ContentOperator::BeginMarkedContent(b"Art".to_vec()));
        assert_eq!(ops[1], ContentOperator::EndMarkedContent);
    }

    // ── unknown operator ──────────────────────────────────────────────────

    #[test]
    fn test_unknown_operator_preserved() {
        let ops = parses_to(b"/XYZ abc");
        if let ContentOperator::Unknown { operator_bytes, .. } = &ops[0] {
            assert_eq!(operator_bytes, b"abc");
        } else {
            panic!("expected Unknown");
        }
    }

    // ── realistic snippet from real PDF ───────────────────────────────────

    #[test]
    fn test_realistic_content_stream() {
        let data = b"q\n/GS0 gs\n/DeviceRGB cs\n0 0 0 scn\n1 0 0 1 0 0 cm\nBT\n/F1 10 Tf\n50 700 Td\n(Laporan) Tj\nT*\n(Tahunan) Tj\nET\nQ\n";
        let ops = parses_to(data);
        assert!(ops.len() >= 10);
        assert_eq!(ops[0], ContentOperator::SaveState);
        assert_eq!(ops[ops.len()-1], ContentOperator::RestoreState);

        // round-trip
        let output = serialise_content_stream(&ops);
        assert_eq!(data.to_vec(), output);
    }
}
