extern crate std;
use std::borrow::ToOwned;

use nanojson::{Serializer, SerializeError, Serialize};
use nanojson::{Parser, Deserialize};
use nanojson::SliceWriter;
use nanojson::{ParseErrorKind, WriteError};

// ============================================================
// Helpers
// ============================================================

fn do_serialize<const DEPTH: usize, F>(buf: &mut [u8], pp: usize, f: F) -> &str
where
    F: FnOnce(&mut Serializer<&mut SliceWriter<'_>, DEPTH>) -> Result<(), SerializeError<WriteError>>,
{
    let mut w = SliceWriter::new(buf);
    let mut ser = Serializer::<_, DEPTH>::with_pp(&mut w, pp);
    f(&mut ser).expect("serialization failed");
    let len = w.pos();
    core::str::from_utf8(&buf[..len]).expect("output is not utf8")
}

macro_rules! ser {
    ($f:expr) => {{
        let mut buf = [0u8; 4096];
        do_serialize::<32, _>(&mut buf, 0, $f).to_owned()
    }};
    (pp=$n:expr, $f:expr) => {{
        let mut buf = [0u8; 4096];
        do_serialize::<32, _>(&mut buf, $n, $f).to_owned()
    }};
}

/// Try to serialize; return `Err(SerializeError)` if it fails.
fn try_serialize<const DEPTH: usize, F>(
    buf: &mut [u8],
    f: F,
) -> Result<usize, SerializeError<WriteError>>
where
    F: FnOnce(&mut Serializer<&mut SliceWriter<'_>, DEPTH>) -> Result<(), SerializeError<WriteError>>,
{
    let mut w = SliceWriter::new(buf);
    let mut json = Serializer::<_, DEPTH>::new(&mut w);
    f(&mut json)?;
    Ok(w.pos())
}

// ============================================================
// ---- Serializer: primitives ----
// ============================================================

#[test]
fn test_null() { assert_eq!(ser!(|j| j.null()), "null"); }

#[test]
fn test_bool() {
    assert_eq!(ser!(|j| j.bool_val(true)),  "true");
    assert_eq!(ser!(|j| j.bool_val(false)), "false");
}

#[test]
fn test_integers() {
    assert_eq!(ser!(|j| j.integer(0)),        "0");
    assert_eq!(ser!(|j| j.integer(42)),        "42");
    assert_eq!(ser!(|j| j.integer(-42)),       "-42");
    assert_eq!(ser!(|j| j.integer(i64::MAX)), "9223372036854775807");
    assert_eq!(ser!(|j| j.integer(i64::MIN)), "-9223372036854775808");
    assert_eq!(ser!(|j| j.integer(1_000_000)), "1000000");
}

#[test]
fn test_all_integer_types() {
    assert_eq!(ser!(|j| 0u8.serialize(j)),     "0");
    assert_eq!(ser!(|j| 255u8.serialize(j)),   "255");
    assert_eq!(ser!(|j| (-128i8).serialize(j)), "-128");
    assert_eq!(ser!(|j| 127i8.serialize(j)),   "127");
    assert_eq!(ser!(|j| 65535u16.serialize(j)), "65535");
    assert_eq!(ser!(|j| u32::MAX.serialize(j)), "4294967295");
    // Note: u64 values beyond i64::MAX serialize via i64 cast (wrapping).
    // Use number_raw() for u64::MAX if needed.
}

#[test]
fn test_number_raw() {
    assert_eq!(ser!(|j| j.number_raw("3.14")),   "3.14");
    assert_eq!(ser!(|j| j.number_raw("-0.5")),   "-0.5");
    assert_eq!(ser!(|j| j.number_raw("1e10")),   "1e10");
    assert_eq!(ser!(|j| j.number_raw("1.5e-3")), "1.5e-3");
    assert_eq!(ser!(|j| j.number_raw("0")),       "0");
}

// ============================================================
// ---- Serializer: strings and escaping ----
// ============================================================

#[test]
fn test_strings() {
    assert_eq!(ser!(|j| j.string("")),       "\"\"");
    assert_eq!(ser!(|j| j.string("hello")),  "\"hello\"");
    assert_eq!(ser!(|j| j.string("a\"b")),  r#""a\"b""#);
    assert_eq!(ser!(|j| j.string("a\\b")),  r#""a\\b""#);
    assert_eq!(ser!(|j| j.string("a\nb")),  "\"a\\nb\"");
    assert_eq!(ser!(|j| j.string("a\tb")),  "\"a\\tb\"");
    assert_eq!(ser!(|j| j.string("a\rb")),  "\"a\\rb\"");
}

#[test]
fn test_string_all_control_escapes() {
    // \b \t \n \v \f \r
    assert_eq!(ser!(|j| j.string_bytes(&[0x08])), "\"\\b\"");
    assert_eq!(ser!(|j| j.string_bytes(&[0x09])), "\"\\t\"");
    assert_eq!(ser!(|j| j.string_bytes(&[0x0A])), "\"\\n\"");
    assert_eq!(ser!(|j| j.string_bytes(&[0x0B])), "\"\\v\"");
    assert_eq!(ser!(|j| j.string_bytes(&[0x0C])), "\"\\f\"");
    assert_eq!(ser!(|j| j.string_bytes(&[0x0D])), "\"\\r\"");
    // Low non-printable bytes → \u00XX
    assert_eq!(ser!(|j| j.string_bytes(&[0x01])), "\"\\u0001\"");
    assert_eq!(ser!(|j| j.string_bytes(&[0x1F])), "\"\\u001f\"");
}

#[test]
fn test_string_non_ascii() {
    // Multi-byte UTF-8 passthrough
    assert_eq!(ser!(|j| j.string("café")),    "\"café\"");
    assert_eq!(ser!(|j| j.string("日本語")),   "\"日本語\"");
    assert_eq!(ser!(|j| j.string("→")),       "\"→\"");
    assert_eq!(ser!(|j| j.string("𝄞")),       "\"𝄞\"");  // 4-byte codepoint
}

#[test]
fn test_string_mixed_content() {
    // Mix of printable ASCII, control chars, and UTF-8
    let s = "hi\n\"world\"\t→";
    let got = ser!(|j| j.string(s));
    assert_eq!(got, "\"hi\\n\\\"world\\\"\\t→\"");
}

#[test]
fn test_string_with_special_json_chars_as_key() {
    let s = ser!(|j| {
        j.object_begin()?;
        j.member_key("key/with/slashes")?; j.integer(1)?;
        j.member_key("key\"with\"quotes")?; j.integer(2)?;
        j.object_end()
    });
    assert!(s.contains(r#""key/with/slashes""#));
    assert!(s.contains(r#""key\"with\"quotes""#));
}

// ============================================================
// ---- Serializer: arrays and objects ----
// ============================================================

#[test]
fn test_empty_array() {
    assert_eq!(ser!(|j| { j.array_begin()?; j.array_end() }), "[]");
}

#[test]
fn test_empty_object() {
    assert_eq!(ser!(|j| { j.object_begin()?; j.object_end() }), "{}");
}

#[test]
fn test_array_single() {
    assert_eq!(ser!(|j| { j.array_begin()?; j.integer(42)?; j.array_end() }), "[42]");
}

#[test]
fn test_array_mixed_types() {
    let s = ser!(|j| {
        j.array_begin()?;
        j.null()?;
        j.bool_val(true)?;
        j.integer(-1)?;
        j.string("hi")?;
        j.array_end()
    });
    assert_eq!(s, r#"[null,true,-1,"hi"]"#);
}

#[test]
fn test_object() {
    let s = ser!(|j| {
        j.object_begin()?;
        j.member_key("x")?; j.integer(1)?;
        j.member_key("y")?; j.integer(2)?;
        j.object_end()
    });
    assert_eq!(s, r#"{"x":1,"y":2}"#);
}

#[test]
fn test_nested_array_of_objects() {
    let s = ser!(|j| {
        j.array_begin()?;
        for i in 0i64..3 {
            j.object_begin()?;
            j.member_key("i")?; j.integer(i)?;
            j.member_key("sq")?; j.integer(i * i)?;
            j.object_end()?;
        }
        j.array_end()
    });
    assert_eq!(s, r#"[{"i":0,"sq":0},{"i":1,"sq":1},{"i":2,"sq":4}]"#);
}

#[test]
fn test_array_of_arrays() {
    let s = ser!(|j| {
        j.array_begin()?;
        for row in [[1i64, 2], [3, 4], [5, 6]] {
            j.array_begin()?;
            j.integer(row[0])?;
            j.integer(row[1])?;
            j.array_end()?;
        }
        j.array_end()
    });
    assert_eq!(s, "[[1,2],[3,4],[5,6]]");
}

#[test]
fn test_deeply_nested_object() {
    // Object inside object inside object inside object
    let s = ser!(|j| {
        j.object_begin()?;
        j.member_key("a")?;
        j.object_begin()?;
        j.member_key("b")?;
        j.object_begin()?;
        j.member_key("c")?;
        j.object_begin()?;
        j.member_key("d")?; j.integer(42)?;
        j.object_end()?;
        j.object_end()?;
        j.object_end()?;
        j.object_end()
    });
    assert_eq!(s, r#"{"a":{"b":{"c":{"d":42}}}}"#);
}

#[test]
fn test_recursive_tree_serialize() {
    // Serialize a binary tree (fixed structure, no alloc needed)
    // Tree:     1
    //          / \
    //         2   3
    //        / \
    //       4   5
    fn write_node(j: &mut Serializer<&mut SliceWriter<'_>, 32>, v: i64, left: Option<(i64, i64, i64)>, right: Option<i64>)
        -> Result<(), SerializeError<WriteError>>
    {
        j.object_begin()?;
        j.member_key("v")?; j.integer(v)?;
        j.member_key("l")?;
        if let Some((lv, ll, lr)) = left {
            j.object_begin()?;
            j.member_key("v")?; j.integer(lv)?;
            j.member_key("l")?; j.integer(ll)?;
            j.member_key("r")?; j.integer(lr)?;
            j.object_end()?;
        } else {
            j.null()?;
        }
        j.member_key("r")?;
        if let Some(rv) = right {
            j.integer(rv)?;
        } else {
            j.null()?;
        }
        j.object_end()
    }

    let mut buf = [0u8; 512];
    let s = do_serialize::<32, _>(&mut buf, 0, |j| {
        write_node(j, 1, Some((2, 4, 5)), Some(3))
    });
    assert_eq!(s, r#"{"v":1,"l":{"v":2,"l":4,"r":5},"r":3}"#);
}

// ============================================================
// ---- Serializer: pretty print ----
// ============================================================

#[test]
fn test_pretty_print_2space() {
    let s = ser!(pp=2, |j: &mut Serializer<&mut SliceWriter<'_>, 32>| {
        j.object_begin()?;
        j.member_key("a")?; j.integer(1)?;
        j.member_key("b")?;
        j.array_begin()?;
        j.integer(2)?; j.integer(3)?;
        j.array_end()?;
        j.object_end()
    });
    assert_eq!(s, "{\n  \"a\": 1,\n  \"b\": [\n    2,\n    3\n  ]\n}");
}

#[test]
fn test_pretty_print_4space() {
    let s = ser!(pp=4, |j: &mut Serializer<&mut SliceWriter<'_>, 32>| {
        j.array_begin()?;
        j.object_begin()?;
        j.member_key("name")?; j.string("Alice")?;
        j.member_key("age")?;  j.integer(30)?;
        j.object_end()?;
        j.object_begin()?;
        j.member_key("name")?; j.string("Bob")?;
        j.member_key("age")?;  j.integer(25)?;
        j.object_end()?;
        j.array_end()
    });
    let expected = concat!(
        "[\n",
        "    {\n",
        "        \"name\": \"Alice\",\n",
        "        \"age\": 30\n",
        "    },\n",
        "    {\n",
        "        \"name\": \"Bob\",\n",
        "        \"age\": 25\n",
        "    }\n",
        "]"
    );
    assert_eq!(s, expected);
}

#[test]
fn test_pretty_print_empty_containers() {
    let s = ser!(pp=2, |j: &mut Serializer<&mut SliceWriter<'_>, 32>| {
        j.object_begin()?;
        j.member_key("arr")?; j.array_begin()?;  j.array_end()?;
        j.member_key("obj")?; j.object_begin()?; j.object_end()?;
        j.object_end()
    });
    assert_eq!(s, "{\n  \"arr\": [],\n  \"obj\": {}\n}");
}

// ============================================================
// ---- Serializer: error cases ----
// ============================================================

#[test]
fn test_buffer_full_error() {
    let mut buf = [0u8; 3]; // too small for "null"
    let mut w = SliceWriter::new(&mut buf);
    let mut json: Serializer<_, 32> = Serializer::new(&mut w);
    assert!(matches!(json.null(), Err(SerializeError::Write(WriteError::BufferFull))));
}

#[test]
fn test_buffer_full_mid_object() {
    // Buffer only fits `{"x"` but not the value
    let mut buf = [0u8; 6];
    let result = try_serialize::<32, _>(&mut buf, |j| {
        j.object_begin()?;
        j.member_key("x")?;
        j.integer(12345) // this should overflow the 6-byte buffer
    });
    assert!(matches!(result, Err(SerializeError::Write(WriteError::BufferFull))));
}

#[test]
fn test_member_key_outside_object() {
    // member_key at root level (no scope) should return InvalidState
    let mut buf = [0u8; 64];
    let mut w = SliceWriter::new(&mut buf);
    let mut json: Serializer<_, 32> = Serializer::new(&mut w);
    assert!(matches!(json.member_key("x"), Err(SerializeError::InvalidState)));
}

#[test]
fn test_member_key_inside_array() {
    // member_key inside an array scope should return InvalidState
    let result = try_serialize::<32, _>(&mut [0u8; 64], |j| {
        j.array_begin()?;
        j.member_key("x")
    });
    assert!(matches!(result, Err(SerializeError::InvalidState)));
}

#[test]
fn test_member_key_called_twice() {
    // calling member_key twice without an intervening value should return InvalidState
    let result = try_serialize::<32, _>(&mut [0u8; 64], |j| {
        j.object_begin()?;
        j.member_key("x")?;
        j.member_key("y") // second key without a value
    });
    assert!(matches!(result, Err(SerializeError::InvalidState)));
}

#[test]
fn test_depth_exceeded() {
    // DEPTH=3: can nest at most 3 levels
    let mut buf = [0u8; 256];
    let result = try_serialize::<3, _>(&mut buf, |j| {
        j.array_begin()?; // depth 1
        j.array_begin()?; // depth 2
        j.array_begin()?; // depth 3
        j.array_begin()   // depth 4 → DepthExceeded
    });
    assert!(matches!(result, Err(SerializeError::DepthExceeded)));
}

// ============================================================
// ---- Serializer: Serialize trait ----
// ============================================================

#[test]
fn test_serialize_trait_primitives() {
    assert_eq!(ser!(|j| true.serialize(j)),   "true");
    assert_eq!(ser!(|j| false.serialize(j)),  "false");
    assert_eq!(ser!(|j| 42u32.serialize(j)),  "42");
    assert_eq!(ser!(|j| (-1i8).serialize(j)), "-1");
    assert_eq!(ser!(|j| "hi".serialize(j)),   "\"hi\"");
    assert_eq!(ser!(|j| ().serialize(j)),      "null");
}

#[test]
fn test_serialize_trait_option() {
    let n: Option<i64> = None;
    assert_eq!(ser!(|j| n.serialize(j)), "null");
    let s: Option<i64> = Some(7);
    assert_eq!(ser!(|j| s.serialize(j)), "7");
    let nested: Option<Option<i64>> = Some(None);
    assert_eq!(ser!(|j| nested.serialize(j)), "null");
}

#[test]
fn test_serialize_trait_array() {
    let arr = [1i64, 2, 3];
    assert_eq!(ser!(|j| arr.serialize(j)), "[1,2,3]");
    let empty: [i64; 0] = [];
    assert_eq!(ser!(|j| empty.serialize(j)), "[]");
}

// ============================================================
// ---- Deserializer: valid cases ----
// ============================================================

#[test]
fn test_parse_null() {
    let mut buf = [0u8; 8];
    Parser::new(b"null", &mut buf).null().unwrap();
}

#[test]
fn test_parse_bool() {
    let mut buf = [0u8; 8];
    assert!( Parser::new(b"true",  &mut buf).bool_val().unwrap());
    assert!(!Parser::new(b"false", &mut buf).bool_val().unwrap());
}

#[test]
fn test_parse_number_str_variants() {
    let mut buf = [0u8; 8];
    assert_eq!(Parser::new(b"0",      &mut buf).number_str().unwrap(), "0");
    assert_eq!(Parser::new(b"3.14",   &mut buf).number_str().unwrap(), "3.14");
    assert_eq!(Parser::new(b"-42",    &mut buf).number_str().unwrap(), "-42");
    assert_eq!(Parser::new(b"1e10",   &mut buf).number_str().unwrap(), "1e10");
    assert_eq!(Parser::new(b"1.5e-3", &mut buf).number_str().unwrap(), "1.5e-3");
    assert_eq!(Parser::new(b"-0.0",   &mut buf).number_str().unwrap(), "-0.0");
    assert_eq!(Parser::new(b"1E+2",   &mut buf).number_str().unwrap(), "1E+2");
}

#[test]
fn test_parse_string_escapes() {
    let mut buf = [0u8; 64];
    // All supported escape sequences
    assert_eq!(Parser::new(b"\"\\b\"", &mut buf).string().unwrap(), "\x08");
    assert_eq!(Parser::new(b"\"\\t\"", &mut buf).string().unwrap(), "\t");
    assert_eq!(Parser::new(b"\"\\n\"", &mut buf).string().unwrap(), "\n");
    assert_eq!(Parser::new(b"\"\\r\"", &mut buf).string().unwrap(), "\r");
    assert_eq!(Parser::new(b"\"\\f\"", &mut buf).string().unwrap(), "\x0C");
    assert_eq!(Parser::new(b"\"\\v\"", &mut buf).string().unwrap(), "\x0B");
    assert_eq!(Parser::new(b"\"\\\\\"",&mut buf).string().unwrap(), "\\");
    assert_eq!(Parser::new(b"\"\\\"\"",&mut buf).string().unwrap(), "\"");
    assert_eq!(Parser::new(b"\"\\/\"", &mut buf).string().unwrap(), "/");
}

#[test]
fn test_parse_string_multiple_escapes() {
    let mut buf = [0u8; 64];
    // "a\nb\tc"
    assert_eq!(
        Parser::new(b"\"a\\nb\\tc\"", &mut buf).string().unwrap(),
        "a\nb\tc"
    );
    // "\"quoted\""
    assert_eq!(
        Parser::new(b"\"\\\"quoted\\\"\"", &mut buf).string().unwrap(),
        "\"quoted\""
    );
}

#[test]
fn test_parse_string_utf8() {
    let mut buf = [0u8; 64];
    assert_eq!(Parser::new("\"café\"".as_bytes(), &mut buf).string().unwrap(), "café");
    assert_eq!(Parser::new("\"日本\"".as_bytes(), &mut buf).string().unwrap(), "日本");
}

#[test]
fn test_parse_whitespace_everywhere() {
    // JSON with generous whitespace
    let src = b"  {  \"x\"  :  42  ,  \"y\"  :  true  }  ";
    let mut buf = [0u8; 64];
    let mut j = Parser::new(src, &mut buf);
    let mut x = 0i64;
    let mut y = false;
    j.object_begin().unwrap();
    while let Some(key) = j.object_member().unwrap() {
        match key {
            "x" => x = j.number_str().unwrap().parse().unwrap(),
            "y" => y = j.bool_val().unwrap(),
            _ => panic!("unexpected key"),
        }
    }
    j.object_end().unwrap();
    assert_eq!(x, 42);
    assert!(y);
}

#[test]
fn test_parse_whitespace_in_array() {
    let src = b"[ 1 , 2 , 3 ]";
    let mut buf = [0u8; 8];
    let mut j = Parser::new(src, &mut buf);
    let mut v = [0i64; 3];
    j.array_begin().unwrap();
    let mut i = 0;
    while j.array_item().unwrap() {
        v[i] = j.number_str().unwrap().parse().unwrap();
        i += 1;
    }
    j.array_end().unwrap();
    assert_eq!(v, [1, 2, 3]);
}

#[test]
fn test_parse_array_of_objects() {
    let src = br#"[{"a":1,"b":2},{"a":3,"b":4}]"#;
    let mut buf = [0u8; 64];
    let mut j = Parser::new(src, &mut buf);
    let mut results = [(0i64, 0i64); 2];
    j.array_begin().unwrap();
    let mut idx = 0;
    while j.array_item().unwrap() {
        let mut a = 0i64;
        let mut b = 0i64;
        j.object_begin().unwrap();
        while let Some(key) = j.object_member().unwrap() {
            match key {
                "a" => a = j.number_str().unwrap().parse().unwrap(),
                "b" => b = j.number_str().unwrap().parse().unwrap(),
                _   => panic!("unexpected key"),
            }
        }
        j.object_end().unwrap();
        results[idx] = (a, b);
        idx += 1;
    }
    j.array_end().unwrap();
    assert_eq!(results, [(1, 2), (3, 4)]);
}

#[test]
fn test_parse_array_of_arrays() {
    let src = b"[[1,2],[3,4],[5,6]]";
    let mut buf = [0u8; 8];
    let mut j = Parser::new(src, &mut buf);
    let mut grid = [[0i64; 2]; 3];
    j.array_begin().unwrap();
    let mut row = 0;
    while j.array_item().unwrap() {
        j.array_begin().unwrap();
        let mut col = 0;
        while j.array_item().unwrap() {
            grid[row][col] = j.number_str().unwrap().parse().unwrap();
            col += 1;
        }
        j.array_end().unwrap();
        row += 1;
    }
    j.array_end().unwrap();
    assert_eq!(grid, [[1, 2], [3, 4], [5, 6]]);
}

#[test]
fn test_parse_deeply_nested() {
    // {"a":{"b":{"c":{"d":42}}}}
    let src = br#"{"a":{"b":{"c":{"d":42}}}}"#;
    let mut buf = [0u8; 16];
    let mut j = Parser::new(src, &mut buf);
    j.object_begin().unwrap();
    assert_eq!(j.object_member().unwrap(), Some("a"));
    j.object_begin().unwrap();
    assert_eq!(j.object_member().unwrap(), Some("b"));
    j.object_begin().unwrap();
    assert_eq!(j.object_member().unwrap(), Some("c"));
    j.object_begin().unwrap();
    assert_eq!(j.object_member().unwrap(), Some("d"));
    assert_eq!(j.number_str().unwrap().parse::<i64>().unwrap(), 42);
    assert_eq!(j.object_member().unwrap(), None);
    j.object_end().unwrap();
    assert_eq!(j.object_member().unwrap(), None);
    j.object_end().unwrap();
    assert_eq!(j.object_member().unwrap(), None);
    j.object_end().unwrap();
    assert_eq!(j.object_member().unwrap(), None);
    j.object_end().unwrap();
}

#[test]
fn test_parse_null_values_in_object() {
    let src = br#"{"a":null,"b":1}"#;
    let mut buf = [0u8; 16];
    let mut j = Parser::new(src, &mut buf);
    j.object_begin().unwrap();
    while let Some(key) = j.object_member().unwrap() {
        match key {
            "a" => j.null().unwrap(),
            "b" => { j.number_str().unwrap(); }
            _ => panic!(),
        }
    }
    j.object_end().unwrap();
}

#[test]
fn test_parse_empty_object() {
    let src = b"{}";
    let mut buf = [0u8; 8];
    let mut j = Parser::new(src, &mut buf);
    j.object_begin().unwrap();
    assert_eq!(j.object_member().unwrap(), None);
    j.object_end().unwrap();
}

#[test]
fn test_parse_empty_array() {
    let src = b"[]";
    let mut buf = [0u8; 8];
    let mut j = Parser::new(src, &mut buf);
    j.array_begin().unwrap();
    assert!(!j.array_item().unwrap());
    j.array_end().unwrap();
}

#[test]
fn test_parse_object_single_field() {
    let src = br#"{"k":99}"#;
    let mut buf = [0u8; 16];
    let mut j = Parser::new(src, &mut buf);
    j.object_begin().unwrap();
    assert_eq!(j.object_member().unwrap(), Some("k"));
    assert_eq!(j.number_str().unwrap().parse::<i64>().unwrap(), 99);
    assert_eq!(j.object_member().unwrap(), None);
    j.object_end().unwrap();
}

// ============================================================
// ---- Deserializer: recursive parsing (tree) ----
// ============================================================

/// A binary tree stored in a flat arena (no alloc needed).
#[derive(Debug, PartialEq)]
struct Node {
    value: i64,
    left:  i32,  // -1 = null, ≥0 = index into arena
    right: i32,
}

/// Parse a node or null from the current position.
/// Returns the arena index of the newly parsed node, or -1 for null.
/// `arena` is a pre-allocated slice, `count` is how many nodes are used.
fn parse_node<'src, 'buf>(
    j: &mut Parser<'src, 'buf>,
    arena: &mut [Node; 16],
    count: &mut usize,
) -> i32 {
    if j.is_null_ahead() {
        j.null().unwrap();
        return -1;
    }
    let idx = *count;
    *count += 1;
    let mut value = 0i64;
    let mut left = -1i32;
    let mut right = -1i32;
    j.object_begin().unwrap();
    while let Some(key) = j.object_member().unwrap() {
        match key {
            "v" => value = j.number_str().unwrap().parse().unwrap(),
            "l" => left  = parse_node(j, arena, count),
            "r" => right = parse_node(j, arena, count),
            _   => panic!("unexpected tree field: {key}"),
        }
    }
    j.object_end().unwrap();
    arena[idx] = Node { value, left, right };
    idx as i32
}

#[test]
fn test_parse_recursive_tree() {
    // Tree:    1
    //         / \
    //        2   3
    //       / \
    //      4   5
    let src = br#"{"v":1,"l":{"v":2,"l":{"v":4,"l":null,"r":null},"r":{"v":5,"l":null,"r":null}},"r":{"v":3,"l":null,"r":null}}"#;
    let mut buf = [0u8; 32];
    let mut j = Parser::new(src, &mut buf);
    let mut arena = core::array::from_fn(|_| Node { value: 0, left: -1, right: -1 });
    let mut count = 0usize;
    let root = parse_node(&mut j, &mut arena, &mut count);

    assert_eq!(count, 5);
    assert_eq!(root, 0);
    assert_eq!(arena[0].value, 1);
    // left of root is node 1 (value=2)
    let l = arena[0].left as usize;
    assert_eq!(arena[l].value, 2);
    // right of root is a node with value=3
    let r = arena[0].right as usize;
    assert_eq!(arena[r].value, 3);
}

#[test]
fn test_parse_recursive_list() {
    // A JSON array of objects, each with a "next" that is either null or an object
    // Represented as: [1, [2, [3, null]]]  via {"head":1,"tail":...}
    let src = br#"{"head":1,"tail":{"head":2,"tail":{"head":3,"tail":null}}}"#;
    let mut buf = [0u8; 32];
    let mut j = Parser::new(src, &mut buf);

    fn parse_list<'src, 'buf>(j: &mut Parser<'src, 'buf>, out: &mut [i64; 8], n: &mut usize) {
        if j.is_null_ahead() { j.null().unwrap(); return; }
        j.object_begin().unwrap();
        while let Some(key) = j.object_member().unwrap() {
            match key {
                "head" => {
                    out[*n] = j.number_str().unwrap().parse().unwrap();
                    *n += 1;
                }
                "tail" => parse_list(j, out, n),
                _ => panic!("unexpected key"),
            }
        }
        j.object_end().unwrap();
    }

    let mut values = [0i64; 8];
    let mut n = 0usize;
    parse_list(&mut j, &mut values, &mut n);
    assert_eq!(n, 3);
    assert_eq!(&values[..n], &[1, 2, 3]);
}

// ============================================================
// ---- Deserializer: lookahead ----
// ============================================================

#[test]
fn test_lookahead_does_not_advance() {
    let mut buf = [0u8; 16];
    let mut j = Parser::new(b"42", &mut buf);
    // Peeking should not consume
    assert!(j.is_number_ahead());
    assert!(j.is_number_ahead()); // still works
    assert_eq!(j.number_str().unwrap(), "42"); // token is still there
}

#[test]
fn test_lookahead_all_types() {
    let mut buf = [0u8; 16];
    assert!( Parser::new(b"null",     &mut buf).is_null_ahead());
    assert!( Parser::new(b"true",     &mut buf).is_bool_ahead());
    assert!( Parser::new(b"false",    &mut buf).is_bool_ahead());
    assert!( Parser::new(b"42",       &mut buf).is_number_ahead());
    assert!( Parser::new(b"\"hi\"",   &mut buf).is_string_ahead());
    assert!( Parser::new(b"[1]",      &mut buf).is_array_ahead());
    assert!( Parser::new(b"{\"a\":1}", &mut buf).is_object_ahead());
    // Negative checks
    assert!(!Parser::new(b"null",  &mut buf).is_bool_ahead());
    assert!(!Parser::new(b"42",    &mut buf).is_string_ahead());
    assert!(!Parser::new(b"true",  &mut buf).is_null_ahead());
    assert!(!Parser::new(b"\"s\"", &mut buf).is_number_ahead());
}

#[test]
fn test_lookahead_to_branch_on_type() {
    // Use lookahead to handle a field that can be either number or null
    let src_num  = b"42";
    let src_null = b"null";
    let mut buf = [0u8; 8];

    {
        let mut j = Parser::new(src_num, &mut buf);
        let val: Option<i64> = if j.is_null_ahead() {
            j.null().unwrap(); None
        } else {
            Some(j.number_str().unwrap().parse().unwrap())
        };
        assert_eq!(val, Some(42));
    }
    {
        let mut j = Parser::new(src_null, &mut buf);
        let val: Option<i64> = if j.is_null_ahead() {
            j.null().unwrap(); None
        } else {
            Some(j.number_str().unwrap().parse().unwrap())
        };
        assert_eq!(val, None);
    }
}

// ============================================================
// ---- Deserializer: error cases with kind and offset ----
// ============================================================


// Simpler: individual error tests with inline logic.

#[test]
fn test_error_unexpected_eof_in_object() {
    let src = b"{\"x\":";  // truncated after colon
    let mut buf = [0u8; 16];
    let mut j = Parser::new(src, &mut buf);
    j.object_begin().unwrap();
    j.object_member().unwrap(); // key "x"
    // Trying to read a value hits EOF
    let err = j.number_str().unwrap_err();
    assert!(
        matches!(err.kind, ParseErrorKind::UnexpectedEof)
        || matches!(err.kind, ParseErrorKind::UnexpectedToken { .. }),
        "expected EOF or unexpected token, got {:?}", err.kind
    );
}

#[test]
fn test_error_unexpected_eof_empty() {
    let src = b"";
    let mut buf = [0u8; 16];
    let err = Parser::new(src, &mut buf).null().unwrap_err();
    assert!(
        matches!(err.kind, ParseErrorKind::UnexpectedEof)
        || matches!(err.kind, ParseErrorKind::UnexpectedToken { .. })
    );
}

#[test]
fn test_error_unexpected_eof_in_string() {
    let src = b"\"unterminated";
    let mut buf = [0u8; 32];
    let err = Parser::new(src, &mut buf).string().unwrap_err();
    assert!(matches!(err.kind, ParseErrorKind::UnexpectedEof));
}

#[test]
fn test_error_invalid_escape() {
    let src = b"\"\\q\"";  // \q is not a valid escape
    let mut buf = [0u8; 16];
    let err = Parser::new(src, &mut buf).string().unwrap_err();
    assert!(
        matches!(err.kind, ParseErrorKind::InvalidEscape(b'q')),
        "expected InvalidEscape('q'), got {:?}", err.kind
    );
}

#[test]
fn test_error_invalid_escape_offset() {
    // "a\qb" — error is at offset 3 (the 'q' byte after the backslash)
    //           0123456
    //           "a\qb"  → raw bytes: b'"', b'a', b'\\', b'q', b'b', b'"'
    // token_start was 0 (start of string), but error.offset is at the escape char
    let src = b"\"a\\qb\"";
    let mut buf = [0u8; 16];
    let err = Parser::new(src, &mut buf).string().unwrap_err();
    assert!(matches!(err.kind, ParseErrorKind::InvalidEscape(b'q')));
    // offset 3 = the 'q' byte
    assert_eq!(err.offset, 3, "expected offset 3 for 'q', got {}", err.offset);
}

#[test]
fn test_error_wrong_type_number_expected() {
    // Expecting a number but JSON has a string
    // {"x":"oops"} → after parsing key "x", call number_str()
    // src bytes:  0  1  2  3  4  5
    //             {  "  x  "  :  "  o  o  p  s  "  }
    let src = b"{\"x\":\"oops\"}";
    let mut buf = [0u8; 16];
    let mut j = Parser::new(src, &mut buf);
    j.object_begin().unwrap();
    j.object_member().unwrap(); // key "x"
    let err = j.number_str().unwrap_err();
    assert!(
        matches!(err.kind, ParseErrorKind::UnexpectedToken { expected: "number", .. }),
        "got {:?}", err.kind
    );
    // The string "oops" starts at offset 5
    assert_eq!(err.offset, 5);
}

#[test]
fn test_error_wrong_type_bool_expected() {
    let src = b"42";
    let mut buf = [0u8; 8];
    let err = Parser::new(src, &mut buf).bool_val().unwrap_err();
    assert!(
        matches!(err.kind, ParseErrorKind::UnexpectedToken { expected: "boolean", .. }),
        "got {:?}", err.kind
    );
    assert_eq!(err.offset, 0);
}

#[test]
fn test_error_wrong_type_object_expected() {
    let src = b"[1,2]";
    let mut buf = [0u8; 8];
    let err = Parser::new(src, &mut buf).object_begin().unwrap_err();
    assert!(
        matches!(err.kind, ParseErrorKind::UnexpectedToken { expected: "{", .. }),
        "got {:?}", err.kind
    );
    assert_eq!(err.offset, 0);
}

#[test]
fn test_error_wrong_type_array_expected() {
    let src = b"{\"a\":1}";
    let mut buf = [0u8; 16];
    let err = Parser::new(src, &mut buf).array_begin().unwrap_err();
    assert!(
        matches!(err.kind, ParseErrorKind::UnexpectedToken { expected: "[", .. }),
        "got {:?}", err.kind
    );
    assert_eq!(err.offset, 0);
}

#[test]
fn test_error_unexpected_token_in_object() {
    // Missing colon: {"x" 42}
    let src = b"{\"x\" 42}";
    let mut buf = [0u8; 16];
    let mut j = Parser::new(src, &mut buf);
    j.object_begin().unwrap();
    let err = j.object_member().unwrap_err();
    assert!(
        matches!(err.kind, ParseErrorKind::UnexpectedToken { expected: ":", .. }),
        "got {:?}", err.kind
    );
}

#[test]
fn test_error_invalid_token() {
    let src = b"xyz";  // not a valid JSON token
    let mut buf = [0u8; 8];
    let err = Parser::new(src, &mut buf).bool_val().unwrap_err();
    // This should produce UnexpectedToken or a similar error
    assert!(
        matches!(err.kind, ParseErrorKind::UnexpectedToken { .. }),
        "got {:?}", err.kind
    );
}

#[test]
fn test_error_string_buffer_overflow() {
    let src = b"\"a very long string that wont fit\"";
    let mut buf = [0u8; 4]; // too small
    let mut j = Parser::new(src, &mut buf);
    let err = j.string().unwrap_err();
    assert!(matches!(err.kind, ParseErrorKind::StringBufferOverflow));
    assert_eq!(err.offset, 0); // string started at offset 0
}

#[test]
fn test_error_offset_in_array() {
    // [1, "oops"] — we expect a number at position 4
    //  0  1  2  3
    //  [  1  ,  " ...
    let src = b"[1,\"oops\"]";
    let mut buf = [0u8; 16];
    let mut j = Parser::new(src, &mut buf);
    j.array_begin().unwrap();
    j.array_item().unwrap();
    j.number_str().unwrap(); // "1" ok
    j.array_item().unwrap();
    let err = j.number_str().unwrap_err(); // "oops" is not a number
    assert!(matches!(err.kind, ParseErrorKind::UnexpectedToken { expected: "number", .. }));
    assert_eq!(err.offset, 3); // '"' at index 3
}

#[test]
fn test_unknown_field() {
    let mut buf = [0u8; 64];
    let mut j = Parser::new(b"{\"z\":99}", &mut buf);
    j.object_begin().unwrap();
    assert_eq!(j.object_member().unwrap(), Some("z"));
    let err = j.unknown_field();
    assert!(matches!(err.kind, ParseErrorKind::UnknownField { .. }));
}

// ============================================================
// ---- Roundtrip ----
// ============================================================

#[test]
fn test_roundtrip_object() {
    let mut out_buf = [0u8; 1024];
    let w_len;
    {
        let mut w = SliceWriter::new(&mut out_buf);
        let mut json: Serializer<_, 32> = Serializer::new(&mut w);
        json.object_begin().unwrap();
            json.member_key("name").unwrap();   json.string("Alice").unwrap();
            json.member_key("age").unwrap();    json.integer(30).unwrap();
            json.member_key("active").unwrap(); json.bool_val(true).unwrap();
        json.object_end().unwrap();
        w_len = w.pos();
    }

    let mut str_buf = [0u8; 64];
    let mut json = Parser::new(&out_buf[..w_len], &mut str_buf);
    json.object_begin().unwrap();
    let mut name_buf = [0u8; 16]; let mut name_len = 0;
    let mut age = 0i64;
    let mut active = false;
    while let Some(key) = json.object_member().unwrap() {
        match key {
            "name" => {
                let s = json.string().unwrap();
                name_len = s.len();
                name_buf[..name_len].copy_from_slice(s.as_bytes());
            }
            "age"    => { age    = json.number_str().unwrap().parse().unwrap(); }
            "active" => { active = json.bool_val().unwrap(); }
            _        => panic!("unexpected key"),
        }
    }
    json.object_end().unwrap();
    assert_eq!(&name_buf[..name_len], b"Alice");
    assert_eq!(age, 30);
    assert!(active);
}

#[test]
fn test_roundtrip_array_of_numbers() {
    let nums: [i64; 8] = [0, -1, 2, -3, 100, -100, i32::MAX as i64, i32::MIN as i64];
    let mut out = [0u8; 128];
    let w_len;
    {
        let mut w = SliceWriter::new(&mut out);
        let mut json: Serializer<_, 32> = Serializer::new(&mut w);
        json.array_begin().unwrap();
        for &n in &nums { json.integer(n).unwrap(); }
        json.array_end().unwrap();
        w_len = w.pos();
    }
    let mut buf = [0u8; 8];
    let mut j = Parser::new(&out[..w_len], &mut buf);
    let mut parsed = [0i64; 8];
    j.array_begin().unwrap();
    let mut i = 0;
    while j.array_item().unwrap() {
        parsed[i] = j.number_str().unwrap().parse().unwrap();
        i += 1;
    }
    j.array_end().unwrap();
    assert_eq!(parsed, nums);
}

#[test]
fn test_roundtrip_nested_structure() {
    // {"matrix":[[1,2],[3,4]],"label":"grid"}
    let mut out = [0u8; 512];
    let w_len;
    {
        let mut w = SliceWriter::new(&mut out);
        let mut json: Serializer<_, 32> = Serializer::new(&mut w);
        json.object_begin().unwrap();
            json.member_key("matrix").unwrap(); json.array_begin().unwrap();
            for row in [[1i64, 2], [3, 4]] {
                json.array_begin().unwrap();
                for v in row { json.integer(v).unwrap(); }
                json.array_end().unwrap();
            }
            json.array_end().unwrap();
            json.member_key("label").unwrap(); json.string("grid").unwrap();
        json.object_end().unwrap();
        w_len = w.pos();
    }

    let mut buf = [0u8; 32];
    let mut j = Parser::new(&out[..w_len], &mut buf);
    j.object_begin().unwrap();
    let mut matrix = [[0i64; 2]; 2];
    let mut label_buf = [0u8; 8]; let mut label_len = 0;
    while let Some(key) = j.object_member().unwrap() {
        match key {
            "matrix" => {
                j.array_begin().unwrap();
                let mut r = 0;
                while j.array_item().unwrap() {
                    j.array_begin().unwrap();
                    let mut c = 0;
                    while j.array_item().unwrap() {
                        matrix[r][c] = j.number_str().unwrap().parse().unwrap();
                        c += 1;
                    }
                    j.array_end().unwrap();
                    r += 1;
                }
                j.array_end().unwrap();
            }
            "label" => {
                let s = j.string().unwrap();
                label_len = s.len();
                label_buf[..label_len].copy_from_slice(s.as_bytes());
            }
            _ => panic!("unexpected key"),
        }
    }
    j.object_end().unwrap();
    assert_eq!(matrix, [[1, 2], [3, 4]]);
    assert_eq!(&label_buf[..label_len], b"grid");
}

// ============================================================
// ---- Deserialize trait ----
// ============================================================

#[test]
fn test_deserialize_bool() {
    let mut buf = [0u8; 16];
    assert_eq!(bool::deserialize(&mut Parser::new(b"true",  &mut buf)).unwrap(), true);
    assert_eq!(bool::deserialize(&mut Parser::new(b"false", &mut buf)).unwrap(), false);
}

#[test]
fn test_deserialize_integers() {
    let mut buf = [0u8; 8];
    assert_eq!(i64::deserialize(&mut Parser::new(b"42",   &mut buf)).unwrap(), 42i64);
    assert_eq!(i64::deserialize(&mut Parser::new(b"-1",   &mut buf)).unwrap(), -1i64);
    assert_eq!(u32::deserialize(&mut Parser::new(b"255",  &mut buf)).unwrap(), 255u32);
    assert_eq!(i8::deserialize( &mut Parser::new(b"-128", &mut buf)).unwrap(), i8::MIN);
    assert_eq!(i8::deserialize( &mut Parser::new(b"127",  &mut buf)).unwrap(), i8::MAX);
    assert_eq!(u8::deserialize( &mut Parser::new(b"0",    &mut buf)).unwrap(), 0u8);
    assert_eq!(u8::deserialize( &mut Parser::new(b"255",  &mut buf)).unwrap(), 255u8);
}

#[test]
fn test_deserialize_integer_overflow() {
    // 300 doesn't fit in u8
    let mut buf = [0u8; 8];
    let err = u8::deserialize(&mut Parser::new(b"300", &mut buf)).unwrap_err();
    assert!(matches!(err.kind, ParseErrorKind::UnexpectedToken { .. }));
}

#[test]
fn test_deserialize_option() {
    let mut buf = [0u8; 16];
    assert_eq!(Option::<bool>::deserialize(&mut Parser::new(b"null",  &mut buf)).unwrap(), None);
    assert_eq!(Option::<bool>::deserialize(&mut Parser::new(b"false", &mut buf)).unwrap(), Some(false));
    assert_eq!(Option::<i64>::deserialize( &mut Parser::new(b"null",  &mut buf)).unwrap(), None);
    assert_eq!(Option::<i64>::deserialize( &mut Parser::new(b"99",    &mut buf)).unwrap(), Some(99));
}

// ============================================================
// ---- f32 / f64 ----
// ============================================================

#[test]
fn test_f64_serialize() {
    assert_eq!(ser!(|s| 1.5f64.serialize(s)),   "1.5");
    assert_eq!(ser!(|s| (-3.0f64).serialize(s)), "-3");
    assert_eq!(ser!(|s| 0.0f64.serialize(s)),   "0");
    assert_eq!(ser!(|s| 1e10f64.serialize(s)),  "10000000000");
    assert_eq!(ser!(|s| 1.23e-4f64.serialize(s)), "0.000123");
}

#[test]
fn test_f32_serialize() {
    assert_eq!(ser!(|s| 1.5f32.serialize(s)), "1.5");
    assert_eq!(ser!(|s| 0.0f32.serialize(s)), "0");
}

#[test]
fn test_float_non_finite_error() {
    let mut buf = [0u8; 32];
    let r = try_serialize::<32, _>(&mut buf, |s| f64::NAN.serialize(s));
    assert!(matches!(r, Err(SerializeError::InvalidValue(_))));
    let r = try_serialize::<32, _>(&mut buf, |s| f64::INFINITY.serialize(s));
    assert!(matches!(r, Err(SerializeError::InvalidValue(_))));
    let r = try_serialize::<32, _>(&mut buf, |s| f64::NEG_INFINITY.serialize(s));
    assert!(matches!(r, Err(SerializeError::InvalidValue(_))));
}

#[test]
fn test_f64_deserialize() {
    let mut buf = [0u8; 8];
    assert_eq!(f64::deserialize(&mut Parser::new(b"1.5",  &mut buf)).unwrap(), 1.5);
    assert_eq!(f64::deserialize(&mut Parser::new(b"-3.0", &mut buf)).unwrap(), -3.0);
    assert_eq!(f64::deserialize(&mut Parser::new(b"0",    &mut buf)).unwrap(), 0.0);
    assert_eq!(f64::deserialize(&mut Parser::new(b"1e2",  &mut buf)).unwrap(), 100.0);
}

#[test]
fn test_f32_deserialize() {
    let mut buf = [0u8; 8];
    assert!((f32::deserialize(&mut Parser::new(b"1.5", &mut buf)).unwrap() - 1.5f32).abs() < 1e-6);
}

#[cfg(feature = "std")]
#[test]
fn test_f64_roundtrip() {
    for v in [0.0, 1.0, -1.0, 0.5, 1.23456789, -9999.125, 1e15] {
        let json = nanojson::stringify(&v).unwrap();
        let back: f64 = nanojson::parse(&json).unwrap();
        assert!((back - v).abs() <= v.abs() * 1e-10 + 1e-300,
            "{v} -> {json} -> {back}");
    }
}

// ============================================================
// ---- Vec ----
// ============================================================

#[cfg(feature = "alloc")]
#[test]
fn test_vec_serialize() {
    assert_eq!(nanojson::stringify(&vec![1i64, 2, 3]).unwrap(), "[1,2,3]");
    assert_eq!(nanojson::stringify(&Vec::<i64>::new()).unwrap(), "[]");
    assert_eq!(
        nanojson::stringify(&vec!["a".to_owned(), "b".to_owned()]).unwrap(),
        r#"["a","b"]"#
    );
}

#[cfg(feature = "alloc")]
#[test]
fn test_vec_deserialize() {
    let v: std::vec::Vec<i64> = nanojson::parse("[1,2,3]").unwrap();
    assert_eq!(v, [1, 2, 3]);

    let empty: std::vec::Vec<i64> = nanojson::parse("[]").unwrap();
    assert!(empty.is_empty());

    let strings: std::vec::Vec<std::string::String> = nanojson::parse(r#"["x","y"]"#).unwrap();
    assert_eq!(strings, ["x", "y"]);
}

#[cfg(feature = "alloc")]
#[test]
fn test_vec_nested() {
    let v: std::vec::Vec<std::vec::Vec<i64>> = nanojson::parse("[[1,2],[3,4]]").unwrap();
    assert_eq!(v, [[1, 2], [3, 4]]);
}

#[cfg(feature = "alloc")]
#[test]
fn test_vec_roundtrip() {
    let src = vec![10i64, 20, 30];
    let json = nanojson::stringify(&src).unwrap();
    let back: std::vec::Vec<i64> = nanojson::parse(&json).unwrap();
    assert_eq!(src, back);
}

// ============================================================
// ---- Box ----
// ============================================================

#[cfg(feature = "alloc")]
#[test]
fn test_box_serialize() {
    assert_eq!(nanojson::stringify(&std::boxed::Box::new(42i64)).unwrap(), "42");
    assert_eq!(nanojson::stringify(&std::boxed::Box::new(true)).unwrap(), "true");
}

#[cfg(feature = "alloc")]
#[test]
fn test_box_deserialize() {
    let b: std::boxed::Box<i64> = nanojson::parse("42").unwrap();
    assert_eq!(*b, 42);
}

#[cfg(feature = "alloc")]
#[test]
fn test_box_roundtrip() {
    let src = std::boxed::Box::new(vec![1i64, 2, 3]);
    let json = nanojson::stringify(&src).unwrap();
    let back: std::boxed::Box<std::vec::Vec<i64>> = nanojson::parse(&json).unwrap();
    assert_eq!(*src, *back);
}

// ============================================================
// ---- BTreeMap ----
// ============================================================

#[cfg(feature = "alloc")]
#[test]
fn test_btreemap_serialize() {
    let mut m = std::collections::BTreeMap::new();
    m.insert("b".to_owned(), 2i64);
    m.insert("a".to_owned(), 1i64);
    // BTreeMap iterates in sorted key order, so output is deterministic.
    assert_eq!(nanojson::stringify(&m).unwrap(), r#"{"a":1,"b":2}"#);
}

#[cfg(feature = "alloc")]
#[test]
fn test_btreemap_deserialize() {
    let m: std::collections::BTreeMap<std::string::String, i64> =
        nanojson::parse(r#"{"x":1,"y":2}"#).unwrap();
    assert_eq!(m["x"], 1);
    assert_eq!(m["y"], 2);
    assert_eq!(m.len(), 2);
}

#[cfg(feature = "alloc")]
#[test]
fn test_btreemap_empty() {
    let m: std::collections::BTreeMap<std::string::String, i64> =
        nanojson::parse("{}").unwrap();
    assert!(m.is_empty());
}

#[cfg(feature = "alloc")]
#[test]
fn test_btreemap_roundtrip() {
    let mut src = std::collections::BTreeMap::new();
    src.insert("key1".to_owned(), vec![1i64, 2]);
    src.insert("key2".to_owned(), vec![3i64, 4]);
    let json = nanojson::stringify(&src).unwrap();
    let back: std::collections::BTreeMap<std::string::String, std::vec::Vec<i64>> =
        nanojson::parse(&json).unwrap();
    assert_eq!(src, back);
}

// ============================================================
// ---- HashMap ----
// ============================================================

#[cfg(feature = "std")]
#[test]
fn test_hashmap_serialize() {
    let mut m = std::collections::HashMap::new();
    m.insert("only".to_owned(), 99i64);
    let json = nanojson::stringify(&m).unwrap();
    // single-entry map, deterministic
    assert_eq!(json, r#"{"only":99}"#);
}

#[cfg(feature = "std")]
#[test]
fn test_hashmap_deserialize() {
    let m: std::collections::HashMap<std::string::String, bool> =
        nanojson::parse(r#"{"a":true,"b":false}"#).unwrap();
    assert_eq!(m["a"], true);
    assert_eq!(m["b"], false);
}

#[cfg(feature = "std")]
#[test]
fn test_hashmap_roundtrip() {
    let mut src = std::collections::HashMap::new();
    src.insert("x".to_owned(), 1i64);
    src.insert("y".to_owned(), 2i64);
    let json = nanojson::stringify(&src).unwrap();
    let back: std::collections::HashMap<std::string::String, i64> =
        nanojson::parse(&json).unwrap();
    assert_eq!(src, back);
}

// ============================================================
// ---- [T; N] fixed-size arrays ----
// ============================================================

#[test]
fn test_fixed_array_deserialize() {
    let mut buf = [0u8; 8];
    let arr: [i32; 3] = Deserialize::deserialize(
        &mut Parser::new(b"[1,2,3]", &mut buf)
    ).unwrap();
    assert_eq!(arr, [1, 2, 3]);
}

#[test]
fn test_fixed_array_deserialize_empty() {
    let mut buf = [0u8; 8];
    let arr: [i32; 0] = Deserialize::deserialize(
        &mut Parser::new(b"[]", &mut buf)
    ).unwrap();
    assert_eq!(arr, []);
}

#[test]
fn test_fixed_array_too_short() {
    let mut buf = [0u8; 8];
    let r: Result<[i32; 3], _> = Deserialize::deserialize(
        &mut Parser::new(b"[1,2]", &mut buf)
    );
    assert!(matches!(r.unwrap_err().kind,
        ParseErrorKind::UnexpectedToken { expected: "array item", got: "]" }
    ));
}

#[test]
fn test_fixed_array_too_long() {
    let mut buf = [0u8; 8];
    let r: Result<[i32; 2], _> = Deserialize::deserialize(
        &mut Parser::new(b"[1,2,3]", &mut buf)
    );
    assert!(matches!(r.unwrap_err().kind,
        ParseErrorKind::UnexpectedToken { expected: "]", got: "array item" }
    ));
}

#[cfg(feature = "std")]
#[test]
fn test_fixed_array_roundtrip() {
    let src: [i64; 4] = [10, 20, 30, 40];
    let json = nanojson::stringify(&src).unwrap();
    assert_eq!(json, "[10,20,30,40]");
    let back: [i64; 4] = nanojson::parse(&json).unwrap();
    assert_eq!(src, back);
}

#[test]
fn test_fixed_array_nested() {
    let arr: [[i32; 2]; 2] = nanojson::parse_sized::<8, _>(b"[[1,2],[3,4]]").unwrap();
    assert_eq!(arr, [[1, 2], [3, 4]]);
}

// ============================================================
// ---- arrayvec ----
// ============================================================

#[cfg(feature = "arrayvec")]
#[test]
fn test_arrayvec_serialize() {
    let mut v: arrayvec::ArrayVec<i64, 4> = arrayvec::ArrayVec::new();
    v.push(1); v.push(2); v.push(3);
    assert_eq!(nanojson::stringify(&v).unwrap(), "[1,2,3]");

    let empty: arrayvec::ArrayVec<i64, 4> = arrayvec::ArrayVec::new();
    assert_eq!(nanojson::stringify(&empty).unwrap(), "[]");
}

#[cfg(feature = "arrayvec")]
#[test]
fn test_arrayvec_deserialize() {
    let v: arrayvec::ArrayVec<i64, 8> = nanojson::parse("[1,2,3]").unwrap();
    assert_eq!(v.as_slice(), &[1, 2, 3]);
}

#[cfg(feature = "arrayvec")]
#[test]
fn test_arrayvec_overflow_error() {
    let r: Result<arrayvec::ArrayVec<i64, 2>, _> = nanojson::parse("[1,2,3]");
    assert!(r.is_err());
}

#[cfg(feature = "arrayvec")]
#[test]
fn test_arrayvec_roundtrip() {
    let mut src: arrayvec::ArrayVec<i64, 4> = arrayvec::ArrayVec::new();
    src.push(7); src.push(8); src.push(9);
    let json = nanojson::stringify(&src).unwrap();
    let back: arrayvec::ArrayVec<i64, 4> = nanojson::parse(&json).unwrap();
    assert_eq!(src, back);
}

#[cfg(feature = "arrayvec")]
#[test]
fn test_arraystring_serialize() {
    let s = arrayvec::ArrayString::<16>::try_from("hello").unwrap();
    assert_eq!(nanojson::stringify(&s).unwrap(), r#""hello""#);
}

#[cfg(feature = "arrayvec")]
#[test]
fn test_arraystring_deserialize() {
    let s: arrayvec::ArrayString<16> = nanojson::parse(r#""world""#).unwrap();
    assert_eq!(s.as_str(), "world");
}

#[cfg(feature = "arrayvec")]
#[test]
fn test_arraystring_overflow_error() {
    let r: Result<arrayvec::ArrayString<3>, _> = nanojson::parse(r#""toolong""#);
    assert!(r.is_err());
}

#[cfg(feature = "arrayvec")]
#[test]
fn test_arraystring_roundtrip() {
    let src = arrayvec::ArrayString::<16>::try_from("roundtrip").unwrap();
    let json = nanojson::stringify(&src).unwrap();
    let back: arrayvec::ArrayString<16> = nanojson::parse(&json).unwrap();
    assert_eq!(src, back);
}

// ============================================================
// ---- u64 / i128 / u128 serialize (correctness fixes) ----
// ============================================================

#[test]
fn test_u64_large_values() {
    // Values that would wrap/corrupt if cast to i64
    assert_eq!(ser!(|s| u64::MAX.serialize(s)), "18446744073709551615");
    assert_eq!(ser!(|s| (i64::MAX as u64 + 1).serialize(s)), "9223372036854775808");
    assert_eq!(ser!(|s| 0u64.serialize(s)), "0");
}

#[test]
fn test_i128_serialize() {
    assert_eq!(ser!(|s| 0i128.serialize(s)),       "0");
    assert_eq!(ser!(|s| i128::MAX.serialize(s)),   "170141183460469231731687303715884105727");
    assert_eq!(ser!(|s| i128::MIN.serialize(s)),   "-170141183460469231731687303715884105728");
    assert_eq!(ser!(|s| (-1i128).serialize(s)),    "-1");
}

#[test]
fn test_u128_serialize() {
    assert_eq!(ser!(|s| 0u128.serialize(s)),     "0");
    assert_eq!(ser!(|s| u128::MAX.serialize(s)), "340282366920938463463374607431768211455");
}

#[test]
fn test_i128_roundtrip() {
    for v in [0i128, 1, -1, i128::MAX, i128::MIN, 1_000_000_000_000_000_000_000i128] {
        let (buf, len) = nanojson::stringify_sized::<64, _>(&v).unwrap();
        let back: i128 = nanojson::parse_sized::<8, _>(&buf[..len]).unwrap();
        assert_eq!(v, back, "roundtrip failed for {v:?}");
    }
}

#[test]
fn test_u128_roundtrip() {
    for v in [0u128, 1, u128::MAX, u64::MAX as u128 + 1] {
        let (buf, len) = nanojson::stringify_sized::<64, _>(&v).unwrap();
        let back: u128 = nanojson::parse_sized::<8, _>(&buf[..len]).unwrap();
        assert_eq!(v, back, "roundtrip failed for {v:?}");
    }
}
