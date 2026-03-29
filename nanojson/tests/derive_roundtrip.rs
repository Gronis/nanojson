extern crate std;
use std::borrow::ToOwned;
use std::collections::HashMap;

use nanojson::{Serialize, Deserialize, Parser, Serializer, SliceWriter};

// ============================================================
// ---- Test types ----
// ============================================================

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Point {
    x: i64,
    y: i64,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Sensor {
    value: i64,
    active: bool,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Renamed {
    #[nanojson(rename = "first_name")]
    value: i64,
}

// Nested struct: Address contains two i64 fields.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Address {
    street_num: i64,
    zip: i64,
}

// Outer struct wraps inner derived struct.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Person {
    age: i64,
    active: bool,
    address: Address,
}

// Struct with an Option field.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct MaybePoint {
    x: i64,
    y: Option<i64>,
}

// Struct with #[nanojson(default)] fields.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct WithDefaults {
    required: i64,
    #[nanojson(default)]
    count: i64,
    #[nanojson(default)]
    label: ::std::string::String,
    #[nanojson(rename = "is_active", default)]
    active: bool,
}

// Unit enum — serialized as a JSON string.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
enum Direction {
    North,
    South,
    #[nanojson(rename = "east")]
    East,
    West,
}

// Enum with struct variants.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
enum Shape {
    Circle { radius: i64 },
    Rect { width: i64, height: i64 },
}

// Mixed enum: unit variants and struct variants together.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
enum Action {
    Idle,                          // unit variant → "Idle"
    #[nanojson(rename = "quit")]
    Quit,                          // unit variant with rename → "quit"
    Move { x: i64, y: i64 },      // struct variant → {"Move": {"x": …, "y": …}}
    Attack { damage: i64 },
}

// Struct with a &str field — only Serialize is derived.
// Deserialize for &str fields requires manual impl (scratch buffer reuse).
#[derive(Serialize, Debug)]
struct WithLabel<'a> {
    label: &'a str,
    count: i64,
}

// ============================================================
// ---- Helpers ----
// ============================================================

fn serialize_val<T: Serialize>(val: &T) -> std::vec::Vec<u8> {
    let mut buf = [0u8; 1024];
    let written;
    {
        let mut w = SliceWriter::new(&mut buf);
        let mut json: Serializer<_, 32> = Serializer::new(&mut w);
        val.serialize(&mut json).expect("serialization failed");
        written = w.pos();
    }
    buf[..written].to_owned()
}

fn as_str(bytes: &[u8]) -> &str {
    core::str::from_utf8(bytes).unwrap()
}

fn roundtrip<T>(val: &T) -> T
where
    T: Serialize + for<'s, 'b> Deserialize<'s, 'b> + core::fmt::Debug + PartialEq,
{
    let json = serialize_val(val);
    let mut buf = [0u8; 128];
    let mut json = Parser::new(&json);
    T::deserialize(&mut json, &mut buf).expect("deserialization failed")
}

// ============================================================
// ---- Serializer tests ----
// ============================================================

#[test]
fn test_point_serialize() {
    let p = Point { x: 3, y: -7 };
    assert_eq!(as_str(&serialize_val(&p)), r#"{"x":3,"y":-7}"#);
}

#[test]
fn test_sensor_serialize() {
    let s = Sensor { value: 42, active: true };
    assert_eq!(as_str(&serialize_val(&s)), r#"{"value":42,"active":true}"#);
}

#[test]
fn test_renamed_serialize() {
    let r = Renamed { value: 99 };
    let json = serialize_val(&r);
    let s = as_str(&json);
    assert!(s.contains("\"first_name\""), "got: {s}");
    assert!(!s.contains("\"value\""), "got: {s}");
}

#[test]
fn test_with_label_serialize() {
    let w = WithLabel { label: "hello", count: 3 };
    assert_eq!(as_str(&serialize_val(&w)), r#"{"label":"hello","count":3}"#);
}

#[test]
fn test_enum_serialize() {
    assert_eq!(as_str(&serialize_val(&Direction::North)), "\"North\"");
    assert_eq!(as_str(&serialize_val(&Direction::South)), "\"South\"");
    assert_eq!(as_str(&serialize_val(&Direction::East)),  "\"east\"");
    assert_eq!(as_str(&serialize_val(&Direction::West)),  "\"West\"");
}

#[test]
fn test_nested_struct_serialize() {
    let p = Person {
        age: 30,
        active: true,
        address: Address { street_num: 42, zip: 10001 },
    };
    let json = serialize_val(&p);
    let s = as_str(&json);
    assert_eq!(s, r#"{"age":30,"active":true,"address":{"street_num":42,"zip":10001}}"#);
}

#[test]
fn test_option_some_serialize() {
    let mp = MaybePoint { x: 5, y: Some(10) };
    assert_eq!(as_str(&serialize_val(&mp)), r#"{"x":5,"y":10}"#);
}

#[test]
fn test_option_none_serialize() {
    let mp = MaybePoint { x: 5, y: None };
    assert_eq!(as_str(&serialize_val(&mp)), r#"{"x":5,"y":null}"#);
}

#[test]
fn test_struct_variant_serialize() {
    let c = Shape::Circle { radius: 7 };
    assert_eq!(as_str(&serialize_val(&c)), r#"{"Circle":{"radius":7}}"#);

    let r = Shape::Rect { width: 10, height: 20 };
    assert_eq!(as_str(&serialize_val(&r)), r#"{"Rect":{"width":10,"height":20}}"#);
}

// ============================================================
// ---- Roundtrip tests ----
// ============================================================

#[test]
fn test_point_roundtrip() {
    let p = Point { x: 100, y: -200 };
    assert_eq!(p, roundtrip(&p));
}

#[test]
fn test_sensor_roundtrip() {
    let s = Sensor { value: -1, active: false };
    assert_eq!(s, roundtrip(&s));
}

#[test]
fn test_renamed_roundtrip() {
    let r = Renamed { value: 7 };
    assert_eq!(r, roundtrip(&r));
}

#[test]
fn test_nested_struct_roundtrip() {
    let p = Person {
        age: 25,
        active: false,
        address: Address { street_num: 1, zip: 90210 },
    };
    assert_eq!(p, roundtrip(&p));
}

#[test]
fn test_option_some_roundtrip() {
    let mp = MaybePoint { x: 3, y: Some(-99) };
    assert_eq!(mp, roundtrip(&mp));
}

#[test]
fn test_option_none_roundtrip() {
    let mp = MaybePoint { x: 3, y: None };
    assert_eq!(mp, roundtrip(&mp));
}

#[test]
fn test_enum_unit_roundtrip() {
    assert_eq!(Direction::North, roundtrip(&Direction::North));
    assert_eq!(Direction::South, roundtrip(&Direction::South));
    assert_eq!(Direction::East,  roundtrip(&Direction::East));
    assert_eq!(Direction::West,  roundtrip(&Direction::West));
}

#[test]
fn test_struct_variant_roundtrip() {
    assert_eq!(Shape::Circle { radius: 5 }, roundtrip(&Shape::Circle { radius: 5 }));
    assert_eq!(
        Shape::Rect { width: 100, height: 200 },
        roundtrip(&Shape::Rect { width: 100, height: 200 }),
    );
}

// Renamed enum value round-trips: json uses "east", not "East".
#[test]
fn test_renamed_enum_roundtrip() {
    let json = serialize_val(&Direction::East);
    assert_eq!(as_str(&json), "\"east\"");
    let mut buf = [0u8; 32];
    let mut json = Parser::new(&json);
    assert_eq!(Direction::deserialize(&mut json, &mut buf).unwrap(), Direction::East);
}

// ============================================================
// ---- Manual string deserialization ----
// ============================================================

/// Demonstrates the copy-before-overwrite pattern for &str fields.
/// The scratch buffer is reused per string, so callers must copy
/// each string value before calling any further parse methods.
#[test]
fn test_manual_string_deserialize() {
    let src = br#"{"label":"world","count":5}"#;
    let mut str_buf = [0u8; 64];
    let mut json = Parser::new(src);

    let mut label_bytes = [0u8; 16];
    let mut label_len = 0usize;
    let mut count = 0i64;

    json.object_begin().unwrap();
    while let Some(key) = json.object_member(&mut str_buf).unwrap() {
        let k = key.to_owned();
        match k.as_str() {
            "label" => {
                let s = json.string(&mut str_buf).unwrap();
                label_len = s.len();
                label_bytes[..label_len].copy_from_slice(s.as_bytes());
            }
            "count" => {
                count = json.number_str().unwrap().parse().unwrap();
            }
            _ => panic!("unexpected field"),
        }
    }
    json.object_end().unwrap();

    assert_eq!(&label_bytes[..label_len], b"world");
    assert_eq!(count, 5);
}

#[test]
fn test_manual_escaped_string_deserialize() {
    let src = br#"{"label":"hello\nworld","count":1}"#;
    let mut str_buf = [0u8; 64];
    let mut json = Parser::new(src);

    let mut label_bytes = [0u8; 32];
    let mut label_len = 0usize;
    let mut count = 0i64;

    json.object_begin().unwrap();
    while let Some(key) = json.object_member(&mut str_buf).unwrap() {
        let k = key.to_owned();
        match k.as_str() {
            "label" => {
                let s = json.string(&mut str_buf).unwrap();
                label_len = s.len();
                label_bytes[..label_len].copy_from_slice(s.as_bytes());
            }
            "count" => {
                count = json.number_str().unwrap().parse().unwrap();
            }
            _ => panic!("unexpected field"),
        }
    }
    json.object_end().unwrap();

    assert_eq!(&label_bytes[..label_len], b"hello\nworld");
    assert_eq!(count, 1);
}

// ============================================================
// ---- Recursive / nested JSON structures ----
// ============================================================

/// Manually parse an arbitrarily deep nested object (no derive needed).
/// Walks depth-first, counting all integer leaf values found.
fn sum_leaf_integers(json: &mut Parser<'_>, buf: &mut [u8]) -> i64 {
    if json.is_object_ahead() {
        let mut total = 0i64;
        json.object_begin().unwrap();
        while let Some(_key) = json.object_member(buf).unwrap() {
            total += sum_leaf_integers(json, buf);
        }
        json.object_end().unwrap();
        total
    } else if json.is_array_ahead() {
        let mut total = 0i64;
        json.array_begin().unwrap();
        while json.array_item().unwrap() {
            total += sum_leaf_integers(json, buf);
        }
        json.array_end().unwrap();
        total
    } else if json.is_number_ahead() {
        let s = json.number_str().unwrap();
        s.parse::<i64>().unwrap_or(0)
    } else if json.is_null_ahead() {
        json.null().unwrap();
        0
    } else if json.is_bool_ahead() {
        json.bool_val().unwrap();
        0
    } else if json.is_string_ahead() {
        json.string(buf).unwrap();
        0
    } else {
        panic!("unexpected token");
    }
}

#[test]
fn test_recursive_sum_nested() {
    // Sum all integer leaf values in a nested structure.
    let src = br#"{"a":1,"b":{"c":2,"d":{"e":3}},"f":4}"#;
    let mut buf = [0u8; 32];
    let mut json = Parser::new(src);
    assert_eq!(sum_leaf_integers(&mut json, &mut buf), 10); // 1+2+3+4
}

#[test]
fn test_recursive_sum_array_of_objects() {
    let src = br#"[{"x":10,"y":20},{"x":1,"y":2},{"x":100}]"#;
    let mut buf = [0u8; 32];
    let mut json = Parser::new(src);
    assert_eq!(sum_leaf_integers(&mut json, &mut buf), 133); // 10+20+1+2+100
}

#[test]
fn test_recursive_sum_mixed() {
    // Mix of nulls, bools, strings, and integers.
    let src = br#"{"a":null,"b":true,"c":"hello","d":42,"e":[1,2,3]}"#;
    let mut buf = [0u8; 32];
    let mut json = Parser::new(src);
    assert_eq!(sum_leaf_integers(&mut json, &mut buf), 48); // 42+1+2+3
}

/// Serialize a derived struct that itself contains another derived struct,
/// then deserialize the full JSON back with recursive application.
#[test]
fn test_deeply_nested_derived_roundtrip() {
    let p = Person {
        age: 99,
        active: true,
        address: Address { street_num: 123, zip: 99999 },
    };
    let json = serialize_val(&p);
    let s = as_str(&json);
    // Verify raw JSON structure.
    assert!(s.contains("\"age\":99"), "got: {s}");
    assert!(s.contains("\"address\":{\"street_num\":123,\"zip\":99999}"), "got: {s}");
    // Deserialize back.
    let p2 = roundtrip(&p);
    assert_eq!(p, p2);
}

// ============================================================
// ---- Error cases for derived deserializers ----
// ============================================================

#[test]
fn test_missing_field_error() {
    let src = br#"{"x":5}"#;
    let mut buf = [0u8; 64];
    let mut json = Parser::new(src);
    let result = Point::deserialize(&mut json, &mut buf);
    assert!(matches!(
        result,
        Err(nanojson::ParseError { kind: nanojson::ParseErrorKind::MissingField { .. }, .. })
    ));
}

#[test]
fn test_unknown_field_error() {
    let src = br#"{"x":1,"y":2,"z":3}"#;
    let mut buf = [0u8; 64];
    let mut json = Parser::new(src);
    let result = Point::deserialize(&mut json, &mut buf);
    assert!(matches!(
        result,
        Err(nanojson::ParseError { kind: nanojson::ParseErrorKind::UnknownField { .. }, .. })
    ));
}

#[test]
fn test_unknown_field_error_carries_metadata() {
    let src = br#"{"x":1,"y":2,"z":3}"#;
    let mut buf = [0u8; 64];
    let mut json = Parser::new(src);
    let err = Point::deserialize(&mut json, &mut buf).unwrap_err();
    if let nanojson::ParseErrorKind::UnknownField { type_name, expected_fields } = err.kind {
        assert_eq!(type_name, "Point");
        assert!(expected_fields.contains(&"x"));
        assert!(expected_fields.contains(&"y"));
    } else {
        panic!("expected UnknownField error");
    }
}

#[test]
fn test_missing_nested_field_error() {
    // Outer struct present but inner Address is missing zip field.
    let src = br#"{"age":1,"active":true,"address":{"street_num":5}}"#;
    let mut buf = [0u8; 64];
    let mut json = Parser::new(src);
    let result = Person::deserialize(&mut json, &mut buf);
    assert!(matches!(
        result,
        Err(nanojson::ParseError { kind: nanojson::ParseErrorKind::MissingField { .. }, .. })
    ));
}

#[test]
fn test_unknown_field_in_nested_error() {
    let src = br#"{"age":1,"active":false,"address":{"street_num":1,"zip":2,"extra":3}}"#;
    let mut buf = [0u8; 64];
    let mut json = Parser::new(src);
    let result = Person::deserialize(&mut json, &mut buf);
    assert!(matches!(
        result,
        Err(nanojson::ParseError { kind: nanojson::ParseErrorKind::UnknownField { .. }, .. })
    ));
}

// ============================================================
// ---- no_std convenience helpers ----
// ============================================================

#[test]
fn test_stringify_sized_roundtrip() {
    let p = Point { x: 3, y: -7 };
    let mut buf = [0; 128];
    let json = nanojson::stringify_sized(&mut buf, &p).unwrap();
    let p2: Point = nanojson::parse_sized(json, &mut [0; 64]).unwrap();
    assert_eq!(p, p2);
}

#[test]
fn test_stringify_manual_sized() {
    let mut buf = [0; 64];
    let json = nanojson::stringify_manual_sized(&mut buf, |s| {
        s.object_begin()?;
        s.member_key("x")?; s.integer(10)?;
        s.member_key("y")?; s.integer(20)?;
        s.object_end()
    }).unwrap();
    let p: Point = nanojson::parse_sized(json, &mut [0; 32]).unwrap();
    assert_eq!(p, Point { x: 10, y: 20 });
}

#[test]
fn test_measure_matches_stringify_sized() {
    let p = Point { x: 1, y: 2 };
    let mut buf = [0; 128];
    let json = nanojson::stringify_sized(&mut buf, &p).unwrap();
    let measured = nanojson::measure(|s| p.serialize(s));
    assert_eq!(measured, json.len());
}

#[test]
fn test_measure_closure() {
    let n = nanojson::measure(|s| {
        s.object_begin()?;
        s.member_key("x")?; s.integer(42)?;
        s.member_key("y")?; s.integer(-1)?;
        s.object_end()
    });
    // {"x":42,"y":-1} = 15 bytes
    assert_eq!(n, 15);
}

#[test]
fn test_stringify_sized_nested() {
    let person = Person { age: 30, active: true, address: Address { street_num: 1, zip: 99 } };
    let mut buf = [0; 256];
    let json = nanojson::stringify_sized(&mut buf, &person).unwrap();
    let person2: Person = nanojson::parse_sized(json, &mut [0; 64]).unwrap();
    assert_eq!(person, person2);
}

// ============================================================
// ---- std convenience helpers ----
// ============================================================

#[cfg(feature = "std")]
#[test]
fn test_stringify_from_str_roundtrip() {
    let p = Point { x: 100, y: -200 };
    let json = nanojson::stringify(&p).unwrap();
    let p2: Point = nanojson::parse(&json).unwrap();
    assert_eq!(p, p2);
}

#[cfg(feature = "std")]
#[test]
fn test_stringify_from_bytes_roundtrip() {
    let s = Sensor { value: 42, active: false };
    let json = nanojson::stringify(&s).unwrap();
    let s2: Sensor = nanojson::parse_bytes(json.as_bytes()).unwrap();
    assert_eq!(s, s2);
}

#[cfg(feature = "std")]
#[test]
fn test_stringify_manual_closure() {
    let json = nanojson::stringify_manual(|s| {
        s.object_begin()?;
        s.member_key("x")?; s.integer(7)?;
        s.member_key("y")?; s.integer(-2)?;
        s.object_end()
    }).unwrap();
    assert_eq!(json, r#"{"x":7,"y":-2}"#);
    let p: Point = nanojson::parse(&json).unwrap();
    assert_eq!(p, Point { x: 7, y: -2 });
}

#[cfg(feature = "std")]
#[test]
fn test_parse_manual_closure() {
    let src = br#"{"x":3,"y":4}"#;
    let p = nanojson::parse_manual::<Point>(src, |parser, buf| Point::deserialize(parser, buf)).unwrap();
    assert_eq!(p, Point { x: 3, y: 4 });
}

#[test]
fn test_parse_manual_sized_closure() {
    let src = br#"{"x":3,"y":4}"#;
    let p = nanojson::parse_manual_sized::<Point>(src, &mut [0; 256], |parser, buf| Point::deserialize(parser, buf)).unwrap();
    assert_eq!(p, Point { x: 3, y: 4 });
}

#[cfg(feature = "std")]
#[test]
fn test_stringify_nested_struct() {
    let person = Person { age: 25, active: true, address: Address { street_num: 10, zip: 12345 } };
    let json = nanojson::stringify(&person).unwrap();
    let person2: Person = nanojson::parse(&json).unwrap();
    assert_eq!(person, person2);
}

#[cfg(feature = "std")]
#[test]
fn test_stringify_enum() {
    let dir = Direction::East;
    let json = nanojson::stringify(&dir).unwrap();
    assert_eq!(json, "\"east\"");
    let dir2: Direction = nanojson::parse(&json).unwrap();
    assert_eq!(dir, dir2);
}

#[cfg(feature = "std")]
#[test]
fn test_stringify_struct_variant_enum() {
    let shape = Shape::Rect { width: 5, height: 10 };
    let json = nanojson::stringify(&shape).unwrap();
    let shape2: Shape = nanojson::parse(&json).unwrap();
    assert_eq!(shape, shape2);
}

#[cfg(feature = "std")]
#[test]
fn test_measure_matches_stringify_len() {
    let p = Point { x: 99, y: -1 };
    let json = nanojson::stringify(&p).unwrap();
    let measured = nanojson::measure(|s| p.serialize(s));
    assert_eq!(measured, json.len());
}

#[test]
fn test_wrong_type_for_field_error() {
    // x is not an integer.
    let src = br#"{"x":true,"y":2}"#;
    let mut buf = [0u8; 64];
    let mut json = Parser::new(src);
    let result = Point::deserialize(&mut json, &mut buf);
    assert!(matches!(
        result,
        Err(nanojson::ParseError { kind: nanojson::ParseErrorKind::UnexpectedToken { .. }, .. })
    ));
}

#[test]
fn test_enum_unknown_variant_error() {
    let src = br#""InvalidDirection""#;
    let mut buf = [0u8; 64];
    let mut json = Parser::new(src);
    let result = Direction::deserialize(&mut json, &mut buf);
    assert!(matches!(
        result,
        Err(nanojson::ParseError { kind: nanojson::ParseErrorKind::UnknownField { .. }, .. })
    ));
}

#[test]
fn test_struct_enum_unknown_variant_error() {
    let src = br#"{"Triangle":{"base":3,"height":4}}"#;
    let mut buf = [0u8; 64];
    let mut json = Parser::new(src);
    let result = Shape::deserialize(&mut json, &mut buf);
    assert!(matches!(
        result,
        Err(nanojson::ParseError { kind: nanojson::ParseErrorKind::UnknownField { .. }, .. })
    ));
}

// ============================================================
// ---- Mixed enum (unit + struct variants) ----
// ============================================================

#[test]
fn test_mixed_enum_unit_variant_serializes_as_string() {
    assert_eq!(nanojson::stringify(&Action::Idle).unwrap(), r#""Idle""#);
    assert_eq!(nanojson::stringify(&Action::Quit).unwrap(), r#""quit""#);
}

#[test]
fn test_mixed_enum_struct_variant_serializes_as_object() {
    assert_eq!(
        nanojson::stringify(&Action::Move { x: 3, y: -1 }).unwrap(),
        r#"{"Move":{"x":3,"y":-1}}"#
    );
    assert_eq!(
        nanojson::stringify(&Action::Attack { damage: 10 }).unwrap(),
        r#"{"Attack":{"damage":10}}"#
    );
}

#[test]
fn test_mixed_enum_unit_variant_roundtrip() {
    assert_eq!(roundtrip(&Action::Idle), Action::Idle);
    assert_eq!(roundtrip(&Action::Quit), Action::Quit);
}

#[test]
fn test_mixed_enum_struct_variant_roundtrip() {
    assert_eq!(roundtrip(&Action::Move { x: 5, y: 10 }), Action::Move { x: 5, y: 10 });
    assert_eq!(roundtrip(&Action::Attack { damage: 99 }), Action::Attack { damage: 99 });
}

#[test]
fn test_mixed_enum_deserialize_unit_as_object_form() {
    // {"Idle": null} must also be accepted for interop.
    let v: Action = nanojson::parse(r#"{"Idle": null}"#).unwrap();
    assert_eq!(v, Action::Idle);
    let v: Action = nanojson::parse(r#"{"quit": null}"#).unwrap();
    assert_eq!(v, Action::Quit);
}

#[test]
fn test_mixed_enum_struct_variant_as_plain_string_is_error() {
    // "Move" as a plain string should be an error (it needs an object).
    let result: Result<Action, _> = nanojson::parse(r#""Move""#);
    assert!(matches!(
        result,
        Err(nanojson::ParseError { kind: nanojson::ParseErrorKind::UnexpectedToken { .. }, .. })
    ));
}

#[test]
fn test_mixed_enum_unknown_variant_error() {
    let result: Result<Action, _> = nanojson::parse(r#""Unknown""#);
    assert!(matches!(
        result,
        Err(nanojson::ParseError { kind: nanojson::ParseErrorKind::UnknownField { .. }, .. })
    ));
}

#[test]
fn test_mixed_enum_in_vec_roundtrip() {
    let actions = vec![
        Action::Idle,
        Action::Move { x: 1, y: 2 },
        Action::Quit,
        Action::Attack { damage: 5 },
    ];
    let json = nanojson::stringify(&actions).unwrap();
    let back: std::vec::Vec<Action> = nanojson::parse(&json).unwrap();
    assert_eq!(actions, back);
}

#[test]
fn test_option_wrong_type_error() {
    // y field expects i64 or null, but gets a string.
    let src = br#"{"x":1,"y":"not_a_number"}"#;
    let mut buf = [0u8; 64];
    let mut json = Parser::new(src);
    let result = MaybePoint::deserialize(&mut json, &mut buf);
    assert!(matches!(
        result,
        Err(nanojson::ParseError { kind: nanojson::ParseErrorKind::UnexpectedToken { .. }, .. })
    ), "got {:?}", result);
}

#[test]
fn test_error_offset_missing_field() {
    // After parsing x:5 we hit '}' — missing y.
    // The offset should point somewhere in the JSON, not 0.
    let src = br#"{"x":5}"#;
    let mut buf = [0u8; 64];
    let mut json = Parser::new(src);
    let err = Point::deserialize(&mut json, &mut buf).unwrap_err();
    assert!(matches!(err.kind, nanojson::ParseErrorKind::MissingField { .. }));
    // The error offset must be a valid byte position within src.
    assert!(err.offset <= src.len(), "offset {} out of bounds", err.offset);
}

#[test]
fn test_empty_object_missing_all_fields_error() {
    let src = br#"{}"#;
    let mut buf = [0u8; 32];
    let mut json = Parser::new(src);
    let result = Point::deserialize(&mut json, &mut buf);
    assert!(matches!(
        result,
        Err(nanojson::ParseError { kind: nanojson::ParseErrorKind::MissingField { .. }, .. })
    ));
}

// ============================================================
// ---- Edge cases ----
// ============================================================

#[test]
fn test_zero_values_roundtrip() {
    let p = Point { x: 0, y: 0 };
    assert_eq!(p, roundtrip(&p));
}

#[test]
fn test_extreme_values_roundtrip() {
    let p = Point { x: i64::MAX, y: i64::MIN };
    assert_eq!(p, roundtrip(&p));
}

#[test]
fn test_sensor_false_roundtrip() {
    let s = Sensor { value: 0, active: false };
    assert_eq!(s, roundtrip(&s));
}

#[test]
fn test_renamed_negative_roundtrip() {
    let r = Renamed { value: -9999 };
    assert_eq!(r, roundtrip(&r));
}

// ============================================================
// ---- #[nanojson(default)] tests ----
// ============================================================

fn parse_with_defaults(json: &str) -> WithDefaults {
    nanojson::parse(json).expect("parse failed")
}

#[test]
fn test_default_all_absent() {
    // Only the required field is present; the three defaulted fields are absent.
    let v = parse_with_defaults(r#"{"required": 7}"#);
    assert_eq!(v, WithDefaults { required: 7, count: 0, label: String::new(), active: false });
}

#[test]
fn test_default_fields_present() {
    // All fields supplied — values should be taken from JSON, not defaults.
    let v = parse_with_defaults(r#"{"required": 1, "count": 42, "label": "hello", "is_active": true}"#);
    assert_eq!(v, WithDefaults { required: 1, count: 42, label: "hello".to_owned(), active: true });
}

#[test]
fn test_default_partial() {
    // Mix: one default field present, others absent.
    let v = parse_with_defaults(r#"{"required": 5, "count": 99}"#);
    assert_eq!(v, WithDefaults { required: 5, count: 99, label: String::new(), active: false });
}

#[test]
fn test_default_with_rename() {
    // The `active` field is renamed to `is_active` and also has default.
    // Verify rename still works when the field is present.
    let v = parse_with_defaults(r#"{"required": 0, "is_active": true}"#);
    assert!(v.active);
    // Verify the original Rust name is not accepted as a JSON key.
    let result: Result<WithDefaults, _> = nanojson::parse(r#"{"required": 0, "active": true}"#);
    assert!(result.is_err()); // "active" is unknown; only "is_active" is valid
}

#[test]
fn test_default_required_still_required() {
    // The `required` field has no default — omitting it must still error.
    let result: Result<WithDefaults, _> = nanojson::parse(r#"{"count": 1}"#);
    assert!(matches!(
        result,
        Err(nanojson::ParseError { kind: nanojson::ParseErrorKind::MissingField { .. }, .. })
    ));
}

#[test]
fn test_default_roundtrip() {
    let v = WithDefaults { required: 3, count: 10, label: "x".to_owned(), active: true };
    assert_eq!(v, roundtrip(&v));
}

// ============================================================
// ---- Generic field types (HashMap, nested generics) ----
// ============================================================

// Derive must survive fields whose types contain commas inside `<...>`.
#[derive(nanojson::Serialize, nanojson::Deserialize, Debug, PartialEq)]
struct WithMap {
    name: String,
    scores: HashMap<String, i64>,
}

#[derive(nanojson::Serialize, nanojson::Deserialize, Debug, PartialEq)]
struct MultiMap {
    labels: HashMap<String, String>,
    counts: HashMap<String, i64>,
}

// Nested generics: HashMap whose value is a Vec.
#[derive(nanojson::Serialize, nanojson::Deserialize, Debug, PartialEq)]
struct NestedGeneric {
    groups: HashMap<String, Vec<i64>>,
}

#[test]
fn test_derive_hashmap_field() {
    let mut scores = HashMap::new();
    scores.insert("alice".to_owned(), 42i64);
    scores.insert("bob".to_owned(), 7i64);
    let v = WithMap { name: "leaderboard".to_owned(), scores };
    let json = nanojson::stringify(&v).unwrap();
    let back: WithMap = nanojson::parse(&json).unwrap();
    assert_eq!(v, back);
}

#[test]
fn test_derive_multiple_hashmap_fields() {
    let mut labels = HashMap::new();
    labels.insert("env".to_owned(), "prod".to_owned());
    let mut counts = HashMap::new();
    counts.insert("requests".to_owned(), 100i64);
    let v = MultiMap { labels, counts };
    let json = nanojson::stringify(&v).unwrap();
    let back: MultiMap = nanojson::parse(&json).unwrap();
    assert_eq!(v, back);
}

#[test]
fn test_derive_nested_generic_field() {
    let mut groups = HashMap::new();
    groups.insert("odds".to_owned(), vec![1i64, 3, 5]);
    groups.insert("evens".to_owned(), vec![2i64, 4, 6]);
    let v = NestedGeneric { groups };
    let json = nanojson::stringify(&v).unwrap();
    let back: NestedGeneric = nanojson::parse(&json).unwrap();
    assert_eq!(v, back);
}

#[test]
fn test_derive_hashmap_empty() {
    let v = WithMap { name: "empty".to_owned(), scores: HashMap::new() };
    let json = nanojson::stringify(&v).unwrap();
    let back: WithMap = nanojson::parse(&json).unwrap();
    assert_eq!(v, back);
}
