/// Comprehensive RFC 8259 compliance tests for JSON string serialization and parsing.
///
/// Organised in three sections:
///   1. Serializer (encoding)  — `write_string_escaped` / `string_bytes`
///   2. Parser    (decoding)   — `Parser::string`
///   3. Round-trip             — serialize then parse, result must equal input
extern crate std;

use nanojson::{ParseErrorKind, Parser, SerializeError, Serializer, WriteError};
use nanojson::SliceWriter;

// ─── helpers ──────────────────────────────────────────────────────────────────

fn ser(f: impl FnOnce(&mut Serializer<&mut SliceWriter<'_>, 8>) -> Result<(), SerializeError<WriteError>>) -> std::string::String {
    let mut buf = [0u8; 4096];
    let mut w = SliceWriter::new(&mut buf);
    let mut s = Serializer::<_, 8>::new(&mut w);
    f(&mut s).expect("serialization failed");
    let n = w.pos();
    core::str::from_utf8(&buf[..n]).expect("output is not utf-8").to_owned()
}

fn parse<'b>(src: &[u8], buf: &'b mut [u8]) -> Result<&'b str, ParseErrorKind> {
    Parser::new(src).string(buf).map_err(|e| e.kind)
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. SERIALIZER — encoding
// ═══════════════════════════════════════════════════════════════════════════════

// ── control characters 0x00–0x1F ──────────────────────────────────────────────

/// RFC 8259 §7: characters U+0000–U+001F MUST be escaped.
/// Named escapes for the six characters that have them; \uXXXX for the rest.
#[test]
fn ser_all_control_chars() {
    let expected: [&str; 32] = [
        r#""\u0000""#, r#""\u0001""#, r#""\u0002""#, r#""\u0003""#,
        r#""\u0004""#, r#""\u0005""#, r#""\u0006""#, r#""\u0007""#,
        r#""\b""#,     r#""\t""#,     r#""\n""#,     r#""\u000b""#,
        r#""\f""#,     r#""\r""#,     r#""\u000e""#, r#""\u000f""#,
        r#""\u0010""#, r#""\u0011""#, r#""\u0012""#, r#""\u0013""#,
        r#""\u0014""#, r#""\u0015""#, r#""\u0016""#, r#""\u0017""#,
        r#""\u0018""#, r#""\u0019""#, r#""\u001a""#, r#""\u001b""#,
        r#""\u001c""#, r#""\u001d""#, r#""\u001e""#, r#""\u001f""#,
    ];
    for (byte, &want) in (0u8..=0x1Fu8).zip(expected.iter()) {
        let got = ser(|s| s.string_bytes(&[byte]));
        assert_eq!(got, want, "byte 0x{byte:02x}");
    }
}

/// 0x0B (vertical tab) must be \u000b; \v is not a valid JSON escape.
#[test]
fn ser_vt_is_unicode_not_backslash_v() {
    assert_eq!(ser(|s| s.string_bytes(&[0x0B])), r#""\u000b""#);
    assert!(!ser(|s| s.string_bytes(&[0x0B])).contains("\\v"));
}

// ── printable ASCII passthrough ────────────────────────────────────────────────

/// 0x20 (space) and printable chars excluding " (0x22) and \ (0x5C) pass through.
#[test]
fn ser_printable_ascii_passthrough() {
    // Every printable ASCII character that is NOT " or \ must pass through unescaped.
    for byte in 0x20u8..=0x7Eu8 {
        if byte == b'"' || byte == b'\\' { continue; }
        let got = ser(|s| s.string_bytes(&[byte]));
        // Should appear literally inside the quotes, not escaped.
        let inner = &got[1..got.len() - 1]; // strip surrounding "..."
        assert_eq!(inner.as_bytes(), &[byte], "byte 0x{byte:02x} should be literal");
    }
}

/// " must be escaped as \" and \ as \\.
#[test]
fn ser_named_escapes_quote_and_backslash() {
    assert_eq!(ser(|s| s.string_bytes(&[b'"'])),  r#""\"""#);
    assert_eq!(ser(|s| s.string_bytes(&[b'\\'])), r#""\\""#);
}

/// 0x7F (DEL) — the serializer escapes it as \u007f.
/// RFC 8259 only requires escaping 0x00–0x1F, but escaping 0x7F is also legal.
#[test]
fn ser_del_escaped() {
    assert_eq!(ser(|s| s.string_bytes(&[0x7F])), r#""\u007f""#);
}

// ── valid UTF-8 passthrough ────────────────────────────────────────────────────

/// Valid 2-byte UTF-8 sequences pass through unescaped.
#[test]
fn ser_valid_utf8_2byte() {
    // é = U+00E9 = [0xC3, 0xA9]
    assert_eq!(ser(|s| s.string("é")), "\"é\"");
    // ñ = U+00F1 = [0xC3, 0xB1]
    assert_eq!(ser(|s| s.string("ñ")), "\"ñ\"");
}

/// Valid 3-byte UTF-8 sequences pass through unescaped.
#[test]
fn ser_valid_utf8_3byte() {
    // 世 = U+4E16 = [0xE4, 0xB8, 0x96]
    assert_eq!(ser(|s| s.string("世")), "\"世\"");
    // → = U+2192 = [0xE2, 0x86, 0x92]
    assert_eq!(ser(|s| s.string("→")), "\"→\"");
}

/// Valid 4-byte UTF-8 sequences pass through unescaped.
#[test]
fn ser_valid_utf8_4byte() {
    // 𝄞 = U+1D11E = [0xF0, 0x9D, 0x84, 0x9E]
    assert_eq!(ser(|s| s.string("𝄞")), "\"𝄞\"");
    // 😀 = U+1F600 = [0xF0, 0x9F, 0x98, 0x80]
    assert_eq!(ser(|s| s.string("😀")), "\"😀\"");
}

// ── invalid UTF-8 escaping ─────────────────────────────────────────────────────

/// A lone continuation byte (0x80–0xBF) is treated as a single invalid byte
/// and escaped as \u00XX.
#[test]
fn ser_invalid_utf8_lone_continuation() {
    assert_eq!(ser(|s| s.string_bytes(&[0x80])), r#""\u0080""#);
    assert_eq!(ser(|s| s.string_bytes(&[0xBF])), r#""\u00bf""#);
}

/// A byte that can never appear in valid UTF-8 (0xFE, 0xFF) is escaped.
#[test]
fn ser_invalid_utf8_bad_lead() {
    assert_eq!(ser(|s| s.string_bytes(&[0xFE])), r#""\u00fe""#);
    assert_eq!(ser(|s| s.string_bytes(&[0xFF])), r#""\u00ff""#);
}

/// A 2-byte lead with no continuation byte — the lead is escaped, any following
/// byte is reprocessed independently.
#[test]
fn ser_invalid_utf8_truncated_2byte() {
    // [0xC3] alone → "\u00c3"
    assert_eq!(ser(|s| s.string_bytes(&[0xC3])), r#""\u00c3""#);
    // [0xC3, b'A'] → "\u00c3A"  (the ASCII 'A' is a fresh, valid byte)
    assert_eq!(ser(|s| s.string_bytes(&[0xC3, b'A'])), r#""\u00c3A""#);
}

/// 3-byte lead where the first continuation is invalid ASCII (not 10xxxxxx).
/// Only the lead is escaped; the rest is reprocessed.
#[test]
fn ser_invalid_utf8_bad_continuation() {
    // [0xE2, 0x28, 0xA1]: 0xE2 is 3-byte lead, 0x28='(' is ASCII, 0xA1 is lone continuation.
    assert_eq!(ser(|s| s.string_bytes(&[0xE2, 0x28, 0xA1])), r#""\u00e2(\u00a1""#);
}

/// Whatever bytes are given to `string_bytes`, the output must always be valid UTF-8
/// (and therefore valid JSON text).
#[test]
fn ser_output_is_always_valid_utf8() {
    let test_cases: &[&[u8]] = &[
        &[0x80],
        &[0xFF],
        &[0xE2, 0x28, 0xA1],
        &[0xC3],
        &[0x00, 0x01, 0x1F],
        &[0xF8, 0x80, 0x80, 0x80, 0x80],  // 5-byte (invalid)
        b"hello \xFF world",
    ];
    for &input in test_cases {
        let out = ser(|s| s.string_bytes(input));
        assert!(
            core::str::from_utf8(out.as_bytes()).is_ok(),
            "output is not valid UTF-8 for input {:?}: {:?}", input, out
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. PARSER — decoding
// ═══════════════════════════════════════════════════════════════════════════════

// ── named escapes (RFC 8259 §7) ───────────────────────────────────────────────

/// All 8 named escapes defined by RFC 8259 must decode correctly.
#[test]
fn parse_all_named_escapes() {
    let mut buf = [0u8; 64];
    assert_eq!(parse(b"\"\\\"\"", &mut buf).unwrap(), "\"");   // \"  → "
    assert_eq!(parse(b"\"\\\\\"", &mut buf).unwrap(), "\\");   // \\ → backslash
    assert_eq!(parse(b"\"\\/\"", &mut buf).unwrap(),  "/");    // \/  → /
    assert_eq!(parse(b"\"\\b\"", &mut buf).unwrap(),  "\x08"); // \b  → backspace
    assert_eq!(parse(b"\"\\f\"", &mut buf).unwrap(),  "\x0C"); // \f  → form feed
    assert_eq!(parse(b"\"\\n\"", &mut buf).unwrap(),  "\n");   // \n  → newline
    assert_eq!(parse(b"\"\\r\"", &mut buf).unwrap(),  "\r");   // \r  → carriage return
    assert_eq!(parse(b"\"\\t\"", &mut buf).unwrap(),  "\t");   // \t  → tab
}

// ── \uXXXX — BMP codepoints ───────────────────────────────────────────────────

/// \u0000 must decode to the null byte (U+0000).
#[test]
fn parse_u_null() {
    let mut buf = [0u8; 8];
    let s = parse(b"\"\\u0000\"", &mut buf).unwrap();
    assert_eq!(s.as_bytes(), &[0x00]);
}

/// \uXXXX for codepoints in the ASCII range.
#[test]
fn parse_u_ascii() {
    let mut buf = [0u8; 8];
    assert_eq!(parse(b"\"\\u0041\"", &mut buf).unwrap(), "A");      // 'A'
    assert_eq!(parse(b"\"\\u007F\"", &mut buf).unwrap(), "\x7F");   // DEL
    assert_eq!(parse(b"\"\\u0022\"", &mut buf).unwrap(), "\"");     // '"'
    assert_eq!(parse(b"\"\\u005C\"", &mut buf).unwrap(), "\\");     // '\'
}

/// \uXXXX for two-byte UTF-8 codepoints (U+0080–U+07FF).
#[test]
fn parse_u_latin1() {
    let mut buf = [0u8; 8];
    // U+00E9 = é = [0xC3, 0xA9]
    assert_eq!(parse(b"\"\\u00E9\"", &mut buf).unwrap(), "é");
    // U+00F1 = ñ = [0xC3, 0xB1]
    assert_eq!(parse(b"\"\\u00F1\"", &mut buf).unwrap(), "ñ");
    // Lowercase hex digits must also be accepted
    assert_eq!(parse(b"\"\\u00e9\"", &mut buf).unwrap(), "é");
}

/// \uXXXX for three-byte UTF-8 codepoints (U+0800–U+FFFF).
#[test]
fn parse_u_bmp() {
    let mut buf = [0u8; 8];
    // U+4E16 = 世
    assert_eq!(parse(b"\"\\u4E16\"", &mut buf).unwrap(), "世");
    // U+2192 = →
    assert_eq!(parse(b"\"\\u2192\"", &mut buf).unwrap(), "→");
}

/// \uXXXX for the named escapes: \u-form must give the same byte as the named form.
#[test]
fn parse_u_named_equivalence() {
    let mut buf = [0u8; 8];
    assert_eq!(parse(b"\"\\u0008\"", &mut buf).unwrap(), "\x08"); // == \b
    assert_eq!(parse(b"\"\\u0009\"", &mut buf).unwrap(), "\t");   // == \t
    assert_eq!(parse(b"\"\\u000A\"", &mut buf).unwrap(), "\n");   // == \n
    assert_eq!(parse(b"\"\\u000D\"", &mut buf).unwrap(), "\r");   // == \r
    assert_eq!(parse(b"\"\\u000C\"", &mut buf).unwrap(), "\x0C"); // == \f
}

/// A string of pure \uXXXX escapes for ASCII codepoints.
#[test]
fn parse_u_mixed_string() {
    let mut buf = [0u8; 32];
    // \u0048\u0065\u006C\u006C\u006F = "Hello"
    assert_eq!(
        parse(b"\"\\u0048\\u0065\\u006C\\u006C\\u006F\"", &mut buf).unwrap(),
        "Hello"
    );
}

// ── \uXXXX — surrogate pairs (U+10000–U+10FFFF) ───────────────────────────────

/// A surrogate pair decodes to the corresponding supplementary codepoint.
#[test]
fn parse_u_surrogate_pair() {
    let mut buf = [0u8; 8];
    // U+1F600 = 😀.  High surrogate 0xD83D, low surrogate 0xDE00.
    assert_eq!(parse(b"\"\\uD83D\\uDE00\"", &mut buf).unwrap(), "😀");
    // U+1D11E = 𝄞.  High 0xD834, low 0xDD1E.
    assert_eq!(parse(b"\"\\uD834\\uDD1E\"", &mut buf).unwrap(), "𝄞");
}

/// The maximum valid surrogate pair: \uDBFF\uDFFF → U+10FFFF.
#[test]
fn parse_u_surrogate_pair_max() {
    let mut buf = [0u8; 8];
    let s = parse(b"\"\\uDBFF\\uDFFF\"", &mut buf).unwrap();
    // U+10FFFF encodes to [0xF4, 0x8F, 0xBF, 0xBF]
    assert_eq!(s.as_bytes(), &[0xF4, 0x8F, 0xBF, 0xBF]);
}

/// The minimum valid surrogate pair: \uD800\uDC00 → U+10000.
#[test]
fn parse_u_surrogate_pair_min() {
    let mut buf = [0u8; 8];
    let s = parse(b"\"\\uD800\\uDC00\"", &mut buf).unwrap();
    // U+10000 encodes to [0xF0, 0x90, 0x80, 0x80]
    assert_eq!(s.as_bytes(), &[0xF0, 0x90, 0x80, 0x80]);
}

/// Both uppercase and lowercase hex digits are accepted in \uXXXX.
#[test]
fn parse_u_hex_case_insensitive() {
    let mut buf = [0u8; 8];
    assert_eq!(parse(b"\"\\u004e\"", &mut buf).unwrap(), "N");
    assert_eq!(parse(b"\"\\u004E\"", &mut buf).unwrap(), "N");
    assert_eq!(parse(b"\"\\uD83d\\ude00\"", &mut buf).unwrap(), "😀");
    assert_eq!(parse(b"\"\\uD83D\\uDE00\"", &mut buf).unwrap(), "😀");
}

// ── raw UTF-8 in source ────────────────────────────────────────────────────────

/// Raw multi-byte UTF-8 in the JSON source (no escape) is passed through as-is.
#[test]
fn parse_raw_utf8_passthrough() {
    let mut buf = [0u8; 64];
    assert_eq!(parse("\"café\"".as_bytes(), &mut buf).unwrap(), "café");
    assert_eq!(parse("\"日本語\"".as_bytes(), &mut buf).unwrap(), "日本語");
    assert_eq!(parse("\"😀\"".as_bytes(), &mut buf).unwrap(), "😀");
}

// ── error cases ───────────────────────────────────────────────────────────────

/// Unknown escape characters must return `InvalidEscape` with the bad byte.
#[test]
fn parse_err_unknown_escape() {
    let mut buf = [0u8; 16];
    assert!(matches!(parse(b"\"\\q\"", &mut buf), Err(ParseErrorKind::InvalidEscape(b'q'))));
    assert!(matches!(parse(b"\"\\x41\"", &mut buf), Err(ParseErrorKind::InvalidEscape(b'x'))));
    assert!(matches!(parse(b"\"\\a\"", &mut buf), Err(ParseErrorKind::InvalidEscape(b'a'))));
}

/// A lone high surrogate (no following \uDCxx) must fail.
#[test]
fn parse_err_lone_high_surrogate() {
    let mut buf = [0u8; 16];
    // High surrogate followed by end-of-string
    assert!(matches!(
        parse(b"\"\\uD800\"", &mut buf),
        Err(ParseErrorKind::InvalidEscape(b'u'))
    ));
    // High surrogate followed by a non-surrogate \uXXXX
    assert!(matches!(
        parse(b"\"\\uD800\\u0041\"", &mut buf),
        Err(ParseErrorKind::InvalidEscape(b'u'))
    ));
    // High surrogate followed by non-escape character
    assert!(matches!(
        parse(b"\"\\uD800X\"", &mut buf),
        Err(ParseErrorKind::InvalidEscape(b'u'))
    ));
}

/// A lone low surrogate must fail.
#[test]
fn parse_err_lone_low_surrogate() {
    let mut buf = [0u8; 16];
    assert!(matches!(
        parse(b"\"\\uDC00\"", &mut buf),
        Err(ParseErrorKind::InvalidEscape(b'u'))
    ));
    assert!(matches!(
        parse(b"\"\\uDFFF\"", &mut buf),
        Err(ParseErrorKind::InvalidEscape(b'u'))
    ));
}

/// Non-hex characters in \uXXXX must fail with InvalidEscape.
#[test]
fn parse_err_non_hex_in_u_escape() {
    let mut buf = [0u8; 16];
    // 'g' is not a hex digit
    assert!(matches!(
        parse(b"\"\\u004g\"", &mut buf),
        Err(ParseErrorKind::InvalidEscape(b'u'))
    ));
    // Space is not a hex digit
    assert!(matches!(
        parse(b"\"\\u00 0\"", &mut buf),
        Err(ParseErrorKind::InvalidEscape(b'u'))
    ));
}

/// A \u escape with fewer than 4 hex digits before EOF must fail with UnexpectedEof.
#[test]
fn parse_err_truncated_u_escape() {
    let mut buf = [0u8; 16];
    // Only 3 hex digits then EOF
    assert!(matches!(
        parse(b"\"\\u004", &mut buf),
        Err(ParseErrorKind::UnexpectedEof)
    ));
    // Only 2 hex digits then closing quote — the closing " is the 3rd byte,
    // so fewer than 4 bytes remain when we try to read the hex digits → UnexpectedEof.
    assert!(matches!(
        parse(b"\"\\u00\"", &mut buf),
        Err(ParseErrorKind::UnexpectedEof)
    ));
}

/// An unterminated string literal must fail with UnexpectedEof.
#[test]
fn parse_err_eof_in_string() {
    let mut buf = [0u8; 16];
    assert!(matches!(parse(b"\"unterminated", &mut buf), Err(ParseErrorKind::UnexpectedEof)));
    assert!(matches!(parse(b"\"", &mut buf),             Err(ParseErrorKind::UnexpectedEof)));
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. ROUND-TRIP — serialize then parse
// ═══════════════════════════════════════════════════════════════════════════════

/// Every control character 0x00–0x1F, when given as a raw byte to string_bytes,
/// is escaped then parses back to the same byte value.
#[test]
fn roundtrip_control_chars() {
    let mut buf = [0u8; 32];
    for byte in 0x00u8..=0x1Fu8 {
        let encoded = ser(|s| s.string_bytes(&[byte]));
        let decoded = Parser::new(encoded.as_bytes())
            .string(&mut buf)
            .expect(&std::format!("failed to parse encoded byte 0x{byte:02x}: {encoded:?}"));
        assert_eq!(decoded.as_bytes(), &[byte], "round-trip failed for byte 0x{byte:02x}");
    }
}

/// Unicode strings survive a serialize → parse round-trip unchanged.
#[test]
fn roundtrip_unicode_strings() {
    let inputs = ["café", "日本語", "😀", "𝄞", "→", "\u{10FFFF}"];
    let mut buf = [0u8; 64];
    for &s in &inputs {
        let encoded = ser(|ser| ser.string(s));
        let decoded = Parser::new(encoded.as_bytes())
            .string(&mut buf)
            .expect(&std::format!("parse failed for {s:?}: {encoded:?}"));
        assert_eq!(decoded, s, "round-trip failed for {s:?}");
    }
}

/// Strings with special JSON characters survive a round-trip.
#[test]
fn roundtrip_special_ascii() {
    let inputs = ["\"", "\\", "/", "\"hello\\world\"", "a\nb\tc\rd"];
    let mut buf = [0u8; 64];
    for &s in &inputs {
        let encoded = ser(|ser| ser.string(s));
        let decoded = Parser::new(encoded.as_bytes())
            .string(&mut buf)
            .expect(&std::format!("parse failed for {s:?}: {encoded:?}"));
        assert_eq!(decoded, s, "round-trip failed for {s:?}");
    }
}

/// A \uXXXX-escaped string produced by another encoder can be parsed back.
#[test]
fn roundtrip_parse_unicode_escape_encoded_output() {
    // Simulate JSON output from another encoder that uses \uXXXX for everything.
    let mut buf = [0u8; 64];
    // \u0063\u0061\u0066\u00E9 = "café"
    assert_eq!(
        parse(b"\"\\u0063\\u0061\\u0066\\u00E9\"", &mut buf).unwrap(),
        "café"
    );
    // Surrogate pair for 😀 — our serializer doesn't produce these but must parse them.
    assert_eq!(
        parse(b"\"\\uD83D\\uDE00\"", &mut buf).unwrap(),
        "😀"
    );
}
