use nanojson::{ParseError, Parser};

fn parse_pair(source: &[u8]) -> Result<[i64; 2], ParseError> {
    nanojson::parse_sized(&mut [], source)
}

fn parse_object(source: &[u8]) -> Result<(i64, i64), ParseError> {
    nanojson::parse_sized_as(&mut [], source, |parser| {
        parser.object_begin()?;
        let mut a = 0;
        let mut b = 0;
        while let Some(key) = parser.member()? {
            match key {
                "a" => a = parser.integer()?,
                "b" => b = parser.integer()?,
                _ => return Err(parser.unknown_field()),
            }
        }
        parser.object_end()?;
        Ok((a, b))
    })
}

#[test]
fn complete_document_helpers_reject_trailing_content() {
    assert!(nanojson::parse_sized::<i64>(&mut [], b"1 2").is_err());
    assert!(nanojson::parse_sized::<i64>(&mut [], b"1 null").is_err());
    assert!(parse_pair(b"[1,2] {}").is_err());
    assert!(parse_object(br#"{"a":1,"b":2} false"#).is_err());
}

#[test]
fn complete_document_helpers_allow_trailing_json_whitespace() {
    assert_eq!(
        nanojson::parse_sized::<i64>(&mut [], b"1 \t\r\n").unwrap(),
        1
    );
    assert_eq!(parse_pair(b"[1,2] \n").unwrap(), [1, 2]);
}

#[test]
fn arrays_require_exactly_one_comma_between_items() {
    assert!(parse_pair(b"[1 2]").is_err());
    assert!(parse_pair(b"[1,,2]").is_err());
    assert!(parse_pair(b"[,1,2]").is_err());
    assert!(nanojson::parse_sized::<[i64; 1]>(&mut [], b"[1,]").is_err());
    assert_eq!(parse_pair(b"[1,2]").unwrap(), [1, 2]);
}

#[test]
fn objects_require_exactly_one_comma_between_members() {
    assert!(parse_object(br#"{"a":1 "b":2}"#).is_err());
    assert!(parse_object(br#"{"a":1,,"b":2}"#).is_err());
    assert!(parse_object(br#"{,"a":1,"b":2}"#).is_err());
    assert!(parse_object(br#"{"a":1,"b":2,}"#).is_err());
    assert_eq!(parse_object(br#"{"a":1,"b":2}"#).unwrap(), (1, 2));
}

#[test]
fn nested_containers_require_commas() {
    assert!(nanojson::parse_sized::<[[i64; 1]; 2]>(&mut [], b"[[1] [2]]").is_err());
    assert!(parse_object(br#"{"a":{},"b":1}"#).is_err());
}

#[test]
fn number_tokens_follow_json_grammar() {
    for valid in ["0", "-0", "12", "-12", "1.25", "1e2", "1E+2", "-1.5e-2"] {
        assert!(
            nanojson::parse_sized::<f64>(&mut [], valid).is_ok(),
            "{valid}"
        );
    }
    for invalid in ["-", "01", "-01", "1.", "1e", "1e+", ".1", "+1"] {
        assert!(
            nanojson::parse_sized::<f64>(&mut [], invalid).is_err(),
            "{invalid}"
        );
    }
}

#[cfg(feature = "alloc")]
#[test]
fn strings_reject_non_json_controls_and_escapes() {
    let mut scratch = [0; 16];
    assert!(nanojson::parse_sized::<String>(&mut scratch, b"\"line\nfeed\"").is_err());
    assert!(nanojson::parse_sized::<String>(&mut scratch, b"\"tab\tvalue\"").is_err());
    assert!(nanojson::parse_sized::<String>(&mut scratch, b"\"\\v\"").is_err());
    assert_eq!(
        nanojson::parse_sized::<String>(&mut scratch, b"\"\\u000b\"").unwrap(),
        "\u{000b}"
    );
}

#[test]
fn direct_parser_remains_incremental_and_finish_is_explicit() {
    let mut parser = Parser::new(b"1 2", &mut []);
    assert_eq!(parser.integer::<i64>().unwrap(), 1);
    assert!(parser.finish().is_err());

    let mut parser = Parser::new(b"1 2", &mut []);
    assert_eq!(parser.integer::<i64>().unwrap(), 1);
    assert_eq!(parser.integer::<i64>().unwrap(), 2);
    parser.finish().unwrap();
}

#[cfg(feature = "std")]
#[test]
fn heap_allocating_helpers_are_also_strict() {
    assert!(nanojson::parse::<i64>("1 2").is_err());
    assert!(nanojson::parse_as("1 2", |parser| parser.integer::<i64>()).is_err());
    assert_eq!(nanojson::parse::<i64>("1 \n").unwrap(), 1);
}
