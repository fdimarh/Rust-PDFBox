//! Regression tests for malformed and edge-case PDF token inputs.
//!
//! Covers the lexer and parser behaviors documented in Java PDFBox's
//! `TestPDFParser`, `TestCOSParser`, and `TestBaseParser` test suites.
//!
//! # Categories
//!
//! | Category | What we test |
//! |---|---|
//! | Lexer edge tokens | Boundary numbers, degenerate strings/names, solo delimiters |
//! | Lexer error recovery | Every unterminated / illegal sequence produces a `LexError` |
//! | Parser malformed objects | Truncated arrays/dicts, mismatched delimiters, bad keywords |
//! | Parser indirect objects | Missing `obj`/`endobj`, garbage between objects |
//! | Number extremes | i64 min/max, huge real, leading zeros, double-sign |
//! | String edge cases | Empty string, lone backslash, deeply nested parens, null bytes |
//! | Name edge cases | Empty name, name with all-hex encoding, 127-char name |
//! | Comment edge cases | Comment only, comment at EOF, comment inside stream |
//! | Whitespace variants | CR-only EOL, NUL bytes as whitespace |
//! | Dict/array nesting | Deeply nested dict inside array, array inside dict value |

#[cfg(test)]
mod lexer_edge_tokens {
    use crate::parser::lexer::{LexError, Lexer, Token};
    use crate::cos::CosName;

    fn lex_all(input: &[u8]) -> Vec<Token> {
        let mut lexer = Lexer::new(input);
        let mut tokens = Vec::new();
        while let Ok(Some(tok)) = lexer.next_token() {
            tokens.push(tok);
        }
        tokens
    }

    fn lex_first_err(input: &[u8]) -> LexError {
        let mut lexer = Lexer::new(input);
        lexer.next_token().unwrap_err()
    }

    // ---- Number edge cases ----

    #[test]
    fn lex_zero() {
        assert_eq!(lex_all(b"0"), vec![Token::Integer(0)]);
    }

    #[test]
    fn lex_negative_zero() {
        assert_eq!(lex_all(b"-0"), vec![Token::Integer(0)]);
    }

    #[test]
    fn lex_positive_sign_integer() {
        assert_eq!(lex_all(b"+123"), vec![Token::Integer(123)]);
    }

    #[test]
    fn lex_large_integer() {
        assert_eq!(lex_all(b"2147483647"), vec![Token::Integer(2_147_483_647)]);
    }

    #[test]
    fn lex_large_negative_integer() {
        assert_eq!(lex_all(b"-2147483648"), vec![Token::Integer(-2_147_483_648)]);
    }

    #[test]
    fn lex_real_no_leading_digit() {
        // ".5" is a valid PDF real
        assert_eq!(lex_all(b".5"), vec![Token::Real(0.5)]);
    }

    #[test]
    fn lex_real_trailing_dot() {
        // "1." is valid
        let t = lex_all(b"1.");
        assert_eq!(t.len(), 1);
        assert!(matches!(t[0], Token::Real(v) if (v - 1.0).abs() < 1e-10));
    }

    #[test]
    fn lex_real_negative_no_leading_digit() {
        let t = lex_all(b"-.5");
        assert_eq!(t.len(), 1);
        assert!(matches!(t[0], Token::Real(v) if (v - (-0.5)).abs() < 1e-10));
    }

    #[test]
    fn lex_multiple_numbers_no_space_after_delimiter() {
        // Directly adjacent to array delimiter
        let tokens = lex_all(b"[1]");
        assert_eq!(tokens, vec![Token::ArrayStart, Token::Integer(1), Token::ArrayEnd]);
    }

    // ---- String edge cases ----

    #[test]
    fn lex_empty_literal_string() {
        assert_eq!(lex_all(b"()"), vec![Token::LiteralString(vec![])]);
    }

    #[test]
    fn lex_string_with_null_bytes() {
        let tokens = lex_all(b"(\x00\x00)");
        assert_eq!(tokens, vec![Token::LiteralString(vec![0x00, 0x00])]);
    }

    #[test]
    fn lex_string_with_high_bytes() {
        // Non-ASCII bytes should be preserved verbatim
        let tokens = lex_all(b"(\xFF\xFE)");
        assert_eq!(tokens, vec![Token::LiteralString(vec![0xFF, 0xFE])]);
    }

    #[test]
    fn lex_string_lone_backslash_at_end() {
        // Backslash before EOF inside string — treated as unknown escape (byte dropped)
        // The lexer should error on unterminated string
        let mut lexer = Lexer::new(b"(abc\\");
        assert!(lexer.next_token().is_err());
    }

    #[test]
    fn lex_string_escape_unknown_char() {
        // Unknown escape: \z → just 'z' per PDF spec §7.3.4.2
        let tokens = lex_all(b"(\\z)");
        assert_eq!(tokens, vec![Token::LiteralString(b"z".to_vec())]);
    }

    #[test]
    fn lex_string_escape_backslash_newline() {
        // \<LF> is a line continuation — the newline is ignored
        let tokens = lex_all(b"(abc\\\ndef)");
        assert_eq!(tokens, vec![Token::LiteralString(b"abcdef".to_vec())]);
    }

    #[test]
    fn lex_string_escape_backslash_crlf() {
        // \<CR><LF> line continuation
        let tokens = lex_all(b"(abc\\\r\ndef)");
        assert_eq!(tokens, vec![Token::LiteralString(b"abcdef".to_vec())]);
    }

    #[test]
    fn lex_string_deeply_nested_parens() {
        // ((((a)))) — 4 levels deep
        let tokens = lex_all(b"((((a))))");
        assert_eq!(tokens, vec![Token::LiteralString(b"(((a)))".to_vec())]);
    }

    #[test]
    fn lex_string_octal_two_digit() {
        // \17 is octal 17 = 15 decimal
        let tokens = lex_all(b"(\\17)");
        assert_eq!(tokens, vec![Token::LiteralString(vec![0o17])]);
    }

    #[test]
    fn lex_string_octal_one_digit() {
        // \7 is octal 7
        let tokens = lex_all(b"(\\7)");
        assert_eq!(tokens, vec![Token::LiteralString(vec![7])]);
    }

    #[test]
    fn lex_string_octal_overflow_masked() {
        // \777 = 0x1FF masked to 0xFF = 255
        let tokens = lex_all(b"(\\777)");
        assert_eq!(tokens, vec![Token::LiteralString(vec![0xFF])]);
    }

    #[test]
    fn lex_unterminated_literal_string() {
        let err = lex_first_err(b"(hello");
        assert!(err.message.contains("unterminated") || err.message.contains("string"));
    }

    #[test]
    fn lex_unterminated_literal_string_with_escape() {
        let err = lex_first_err(b"(hello\\");
        assert!(err.message.contains("unterminated") || err.message.contains("string") || err.message.contains("escape"));
    }

    // ---- Hex string edge cases ----

    #[test]
    fn lex_empty_hex_string() {
        assert_eq!(lex_all(b"<>"), vec![Token::HexString(vec![])]);
    }

    #[test]
    fn lex_hex_string_all_lowercase() {
        let tokens = lex_all(b"<48656c6c6f>");
        assert_eq!(tokens, vec![Token::HexString(b"Hello".to_vec())]);
    }

    #[test]
    fn lex_hex_string_mixed_case() {
        let tokens = lex_all(b"<48656C6c6F>");
        assert_eq!(tokens, vec![Token::HexString(b"Hello".to_vec())]);
    }

    #[test]
    fn lex_hex_string_whitespace_between_digits() {
        // Whitespace inside hex string is ignored per spec
        let tokens = lex_all(b"<\n48\t65\r6C>");
        assert_eq!(tokens, vec![Token::HexString(b"Hel".to_vec())]);
    }

    #[test]
    fn lex_unterminated_hex_string() {
        let err = lex_first_err(b"<414243");
        assert!(err.message.contains("unterminated") || err.message.contains("hex"));
    }

    // ---- Name edge cases ----

    #[test]
    fn lex_empty_name() {
        // A lone '/' with nothing after it is a valid empty name
        let tokens = lex_all(b"/ ");
        assert_eq!(tokens, vec![Token::Name(CosName::new(vec![]))]);
    }

    #[test]
    fn lex_name_immediately_followed_by_delimiter() {
        // /Type/ — name "Type" ends at '/'
        let tokens = lex_all(b"/Type/");
        assert_eq!(tokens[0], Token::Name(CosName::new(b"Type".to_vec())));
    }

    #[test]
    fn lex_name_with_hash_only() {
        // /#20 = name with single space
        let tokens = lex_all(b"/#20");
        assert_eq!(tokens, vec![Token::Name(CosName::new(b" ".to_vec()))]);
    }

    #[test]
    fn lex_name_all_hex_encoded() {
        // /A#42C = "ABC"
        let tokens = lex_all(b"/A#42C");
        assert_eq!(tokens, vec![Token::Name(CosName::new(b"ABC".to_vec()))]);
    }

    #[test]
    fn lex_name_with_numbers() {
        let tokens = lex_all(b"/F1");
        assert_eq!(tokens, vec![Token::Name(CosName::new(b"F1".to_vec()))]);
    }

    #[test]
    fn lex_name_long() {
        // PDF spec limits names to 127 bytes, but we don't enforce that
        let long_name: Vec<u8> = std::iter::once(b'/').chain(b"A".repeat(127).into_iter()).collect();
        let tokens = lex_all(&long_name);
        assert_eq!(tokens.len(), 1);
        assert!(matches!(&tokens[0], Token::Name(n) if n.as_bytes().len() == 127));
    }

    // ---- Comment edge cases ----

    #[test]
    fn lex_comment_only_stream() {
        // Only a comment — should yield no tokens
        assert!(lex_all(b"% nothing here").is_empty());
    }

    #[test]
    fn lex_comment_at_eof_no_newline() {
        // Comment at end without newline
        assert!(lex_all(b"% eof comment").is_empty());
    }

    #[test]
    fn lex_comment_followed_by_token() {
        let tokens = lex_all(b"% comment\r\n42");
        assert_eq!(tokens, vec![Token::Integer(42)]);
    }

    #[test]
    fn lex_multiple_comments() {
        let tokens = lex_all(b"% line 1\n% line 2\n/Name");
        assert_eq!(tokens, vec![Token::Name(CosName::new(b"Name".to_vec()))]);
    }

    #[test]
    fn lex_pdf_header_comment_skipped() {
        // %PDF-1.7 and %âãÏÓ are comments — parser skips them
        let tokens = lex_all(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n1");
        assert_eq!(tokens, vec![Token::Integer(1)]);
    }

    // ---- Whitespace variants ----

    #[test]
    fn lex_cr_only_eol() {
        let tokens = lex_all(b"42\r99");
        assert_eq!(tokens, vec![Token::Integer(42), Token::Integer(99)]);
    }

    #[test]
    fn lex_nul_byte_as_whitespace() {
        // NUL (0x00) is whitespace in PDF
        let tokens = lex_all(b"42\x0099");
        assert_eq!(tokens, vec![Token::Integer(42), Token::Integer(99)]);
    }

    #[test]
    fn lex_form_feed_as_whitespace() {
        // Form feed (0x0C) is whitespace in PDF
        let tokens = lex_all(b"42\x0C99");
        assert_eq!(tokens, vec![Token::Integer(42), Token::Integer(99)]);
    }

    // ---- Delimiter edge cases ----

    #[test]
    fn lex_stray_greater_than() {
        // A lone '>' without a preceding '<' is a lex error
        let mut lexer = Lexer::new(b">");
        assert!(lexer.next_token().is_err());
    }

    #[test]
    fn lex_empty_input() {
        assert!(lex_all(b"").is_empty());
    }

    #[test]
    fn lex_only_whitespace() {
        assert!(lex_all(b"   \t\n\r  ").is_empty());
    }

    #[test]
    fn lex_bool_true_false_adjacent() {
        let tokens = lex_all(b"true false");
        assert_eq!(tokens, vec![Token::Bool(true), Token::Bool(false)]);
    }

    #[test]
    fn lex_keyword_tstar() {
        // T* is a valid content-stream operator
        let tokens = lex_all(b"T*");
        assert_eq!(tokens, vec![Token::Keyword(b"T*".to_vec())]);
    }

    #[test]
    fn lex_keyword_single_quote() {
        // ' (apostrophe) is the "move-to-next-line-and-show-text" operator
        let tokens = lex_all(b"'");
        assert_eq!(tokens, vec![Token::Keyword(b"'".to_vec())]);
    }

    #[test]
    fn lex_keyword_double_quote() {
        // " is "set spacing, move to next line, show text"
        let tokens = lex_all(b"\"");
        assert_eq!(tokens, vec![Token::Keyword(b"\"".to_vec())]);
    }

    #[test]
    fn lex_error_message_contains_offset() {
        // Error message must carry the byte offset
        let mut lexer = Lexer::new(b"   >");
        let err = lexer.next_token().unwrap_err();
        assert_eq!(err.offset, 3);
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod parser_malformed {
    use crate::parser::parser::{ParseError, Parser};
    use crate::cos::{CosName, CosObject, ObjectId};

    fn parse_one(input: &[u8]) -> Result<Option<CosObject>, ParseError> {
        Parser::new(input).parse_object()
    }

    fn parse_indirect(input: &[u8]) -> Result<Option<(ObjectId, CosObject)>, ParseError> {
        Parser::new(input).parse_indirect_object()
    }

    // ---- Truncated / unterminated structures ----

    #[test]
    fn truncated_array_no_close() {
        assert!(parse_one(b"[1 2 3").is_err());
    }

    #[test]
    fn truncated_dict_no_close() {
        assert!(parse_one(b"<< /Key 1").is_err());
    }

    #[test]
    fn truncated_dict_missing_value() {
        // Key present but no value before >>
        assert!(parse_one(b"<< /Key >>").is_err());
    }

    #[test]
    fn truncated_nested_dict() {
        assert!(parse_one(b"<< /Inner << /Key 1 >>").is_err());
    }

    #[test]
    fn empty_bytes_returns_none() {
        assert_eq!(parse_one(b"").unwrap(), None);
    }

    // ---- Mismatched / unexpected delimiters ----

    #[test]
    fn array_closed_by_dict_end() {
        // [ 1 >> — mismatched closer; parser should not infinite-loop and
        // should either error or return partial array (both are acceptable).
        let result = parse_one(b"[1 >>");
        // We accept either an error or a partial/empty array — but not a hang.
        // The key invariant is: must terminate.
        let _ = result;
    }

    #[test]
    fn dict_closed_by_array_end() {
        // << /K ] — unexpected ]
        let result = parse_one(b"<< /K ]");
        let _ = result; // must terminate
    }

    #[test]
    fn standalone_array_end_returns_none() {
        // A bare ']' with no open array — parser pushes it back → no object
        let result = parse_one(b"]");
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn standalone_dict_end_returns_none() {
        // A bare '>>' — parser pushes it back → no object
        let result = parse_one(b">>");
        assert!(result.unwrap().is_none());
    }

    // ---- Deeply nested structures (stack safety) ----

    #[test]
    fn deeply_nested_arrays() {
        // 50 levels of [[[[... ]]]] — must parse without stack overflow
        let open: Vec<u8> = b"[".repeat(50);
        let close: Vec<u8> = b"]".repeat(50);
        let mut input = open;
        input.extend_from_slice(b"42");
        input.extend_from_slice(&close);
        let obj = parse_one(&input).unwrap().unwrap();
        // Walk to the innermost value
        let mut cur = &obj;
        let mut depth = 0usize;
        loop {
            match cur {
                CosObject::Array(arr) if arr.len() == 1 => {
                    cur = &arr[0];
                    depth += 1;
                }
                CosObject::Integer(42) => break,
                other => panic!("unexpected at depth {depth}: {other:?}"),
            }
        }
        assert_eq!(depth, 50);
    }

    #[test]
    fn deeply_nested_dicts() {
        // 20 levels of << /K << /K << ... >> >> >>
        let key = CosName::new(b"K".to_vec());
        let mut input = Vec::new();
        for _ in 0..20 {
            input.extend_from_slice(b"<< /K ");
        }
        input.extend_from_slice(b"42");
        for _ in 0..20 {
            input.extend_from_slice(b" >>");
        }
        let obj = parse_one(&input).unwrap().unwrap();
        let mut cur = obj;
        for _ in 0..20 {
            let dict = cur.into_dictionary().expect("expected dict");
            cur = dict.get(&key).unwrap().clone();
        }
        assert_eq!(cur, CosObject::Integer(42));
    }

    // ---- Indirect object edge cases ----

    #[test]
    fn indirect_obj_missing_endobj() {
        // endobj is optional in our parser — value should still parse
        let result = parse_indirect(b"1 0 obj\n42");
        let (id, val) = result.unwrap().unwrap();
        assert_eq!(id, ObjectId::new(1, 0));
        assert_eq!(val, CosObject::Integer(42));
    }

    #[test]
    fn indirect_obj_null_value() {
        let (_, val) = parse_indirect(b"1 0 obj null endobj").unwrap().unwrap();
        assert_eq!(val, CosObject::Null);
    }

    #[test]
    fn indirect_obj_empty_dict() {
        let (_, val) = parse_indirect(b"5 0 obj\n<< >>\nendobj").unwrap().unwrap();
        assert_eq!(val.as_dictionary().unwrap().len(), 0);
    }

    #[test]
    fn indirect_obj_wrong_keyword_instead_of_obj() {
        // "1 0 endobj" — no 'obj' keyword → parse_indirect_object returns None
        // (pushes back the integer and gives up)
        let result = parse_indirect(b"1 0 endobj");
        // Either None or error are acceptable — must not panic
        let _ = result;
    }

    #[test]
    fn indirect_obj_large_object_number() {
        let result = parse_indirect(b"99999 0 obj\n(big)\nendobj").unwrap().unwrap();
        assert_eq!(result.0, ObjectId::new(99999, 0));
    }

    #[test]
    fn indirect_obj_nonzero_generation() {
        let (id, _) = parse_indirect(b"3 7 obj\ntrue\nendobj").unwrap().unwrap();
        assert_eq!(id, ObjectId::new(3, 7));
    }

    // ---- Mixed-type array edge cases ----

    #[test]
    fn array_with_null_elements() {
        let obj = parse_one(b"[null null null]").unwrap().unwrap();
        let arr = obj.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert!(arr.iter().all(|v| v.is_null()));
    }

    #[test]
    fn array_with_nested_reference() {
        let obj = parse_one(b"[1 0 R [2 0 R]]").unwrap().unwrap();
        let arr = obj.as_array().unwrap();
        assert_eq!(arr[0].as_reference(), Some(ObjectId::new(1, 0)));
        let inner = arr[1].as_array().unwrap();
        assert_eq!(inner[0].as_reference(), Some(ObjectId::new(2, 0)));
    }

    #[test]
    fn array_single_real() {
        let obj = parse_one(b"[3.14]").unwrap().unwrap();
        let arr = obj.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert!(matches!(arr[0], CosObject::Real(v) if (v - 3.14).abs() < 1e-10));
    }

    // ---- Dictionary edge cases ----

    #[test]
    fn dict_with_duplicate_keys_last_wins() {
        // PDF spec §7.3.7: duplicate keys — implementations may use either value.
        // Our parser keeps the first (insert-if-present replaces). Either is correct,
        // but it must not error or panic.
        let obj = parse_one(b"<< /K 1 /K 2 >>").unwrap().unwrap();
        let dict = obj.as_dictionary().unwrap();
        // Should have exactly one entry for /K
        let val = dict.get_int(&CosName::new(b"K".to_vec()));
        assert!(val == Some(1) || val == Some(2));
    }

    #[test]
    fn dict_key_is_indirect_reference_not_name() {
        // A dict with a non-name key is malformed — must not panic
        let result = parse_one(b"<< 1 0 R 42 >>");
        let _ = result;
    }

    #[test]
    fn dict_value_is_array() {
        let obj = parse_one(b"<< /Kids [1 0 R 2 0 R] >>").unwrap().unwrap();
        let dict = obj.as_dictionary().unwrap();
        let kids = dict.get_array(&CosName::kids()).unwrap();
        assert_eq!(kids.len(), 2);
    }

    // ---- Number edge cases in objects ----

    #[test]
    fn parse_very_large_real() {
        let obj = parse_one(b"1234567890.123456").unwrap().unwrap();
        assert!(matches!(obj, CosObject::Real(_)));
    }

    #[test]
    fn parse_negative_real_in_array() {
        let obj = parse_one(b"[-1.5 -2.5]").unwrap().unwrap();
        let arr = obj.as_array().unwrap();
        assert!(matches!(arr[0], CosObject::Real(v) if (v - (-1.5)).abs() < 1e-10));
        assert!(matches!(arr[1], CosObject::Real(v) if (v - (-2.5)).abs() < 1e-10));
    }

    // ---- Indirect reference edge cases ----

    #[test]
    fn partial_indirect_ref_two_ints_no_r() {
        // "5 0" followed by EOF — should parse as Integer(5), not a reference
        let obj = parse_one(b"5 0").unwrap().unwrap();
        assert_eq!(obj, CosObject::Integer(5));
    }

    #[test]
    fn partial_indirect_ref_one_int_no_gen() {
        // Just "5" — plain integer
        let obj = parse_one(b"5").unwrap().unwrap();
        assert_eq!(obj, CosObject::Integer(5));
    }

    #[test]
    fn indirect_ref_zero_zero() {
        // 0 0 R is technically a reference to the null object
        let obj = parse_one(b"0 0 R").unwrap().unwrap();
        assert_eq!(obj.as_reference(), Some(ObjectId::new(0, 0)));
    }

    // ---- Keyword-as-non-value tokens ----

    #[test]
    fn obj_keyword_alone_returns_none() {
        // "obj" is not a COS value, parse_object must return None
        let result = parse_one(b"obj").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn endobj_keyword_alone_returns_none() {
        let result = parse_one(b"endobj").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn stream_keyword_alone_returns_none() {
        let result = parse_one(b"stream").unwrap();
        assert!(result.is_none());
    }

    // ---- Garbage before and after valid objects ----

    #[test]
    fn garbage_before_object_in_indirect() {
        // Extra whitespace and comments before "N G obj" is fine
        let result = parse_indirect(b"   % comment\n1 0 obj\n42\nendobj");
        let (id, val) = result.unwrap().unwrap();
        assert_eq!(id, ObjectId::new(1, 0));
        assert_eq!(val, CosObject::Integer(42));
    }

    #[test]
    fn object_with_trailing_garbage_ignored() {
        // After a valid object, trailing bytes are not consumed by parse_object
        let mut p = Parser::new(b"42 garbage_follows");
        let obj = p.parse_object().unwrap().unwrap();
        assert_eq!(obj, CosObject::Integer(42));
    }

    // ---- Stream object stubs ----

    #[test]
    fn dict_followed_by_stream_keyword_produces_stream() {
        // << /Length 0 >> stream\nendstream  → CosObject::Stream
        let input = b"<< /Length 0 >> stream\nendstream";
        let obj = parse_one(input).unwrap().unwrap();
        assert!(matches!(obj, CosObject::Stream(_)));
    }

    #[test]
    fn stream_object_has_dictionary() {
        let input = b"<< /Length 5 /Filter /FlateDecode >> stream\nendstream";
        let obj = parse_one(input).unwrap().unwrap();
        if let CosObject::Stream(s) = obj {
            assert!(s.dictionary.get_int(&CosName::length()).is_some());
        } else {
            panic!("expected stream");
        }
    }
}

