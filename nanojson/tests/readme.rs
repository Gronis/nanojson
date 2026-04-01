use core::convert::Infallible;

use nanojson::{ParseError, SerializeError, Serializer, SliceWriter, WriteError};

#[allow(unused)]
#[derive(Debug)]
enum Error {
    ParseError(ParseError),
    SerializeErrorInfallible(SerializeError<Infallible>),
    SerializeErrorWriteError(SerializeError<WriteError>),
}

impl From<ParseError> for Error {
    fn from(e: ParseError) -> Self {
        Error::ParseError(e)
    }
}

impl From<SerializeError<Infallible>> for Error {
    fn from(e: SerializeError<Infallible>) -> Self {
        Error::SerializeErrorInfallible(e)
    }
}
impl From<SerializeError<WriteError>> for Error {
    fn from(e: SerializeError<WriteError>) -> Self {
        Error::SerializeErrorWriteError(e)
    }
}

#[test]
fn test_readme() {
    let result = test_readme_impl();
    assert!(result.is_ok(), "test_readme_impl failed: {:?}", result.err());
}

#[allow(unused)]
fn test_readme_impl() -> Result<(), Error> {

    let entity = Entity { id: 42, active: true, health: 100, position: Vec2 { x: 0, y: 0 } };
    // ------------------------------------------------
    // std tier
    // ------------------------------------------------


    // Serialization
    // One-liner for a derived type
    let json: String = nanojson::stringify(&entity)?;

    // Closure form for hand-written JSON
    let json: String = nanojson::stringify_as(|s| {
        s.object_begin()?;
          s.member("name")?; s.string("Alice")?;
          s.member("age")?;  s.integer(30)?;
        s.object_end()
    })?;

    // Deserialization
    let json = r#"{"id":42,"is_active":true,"position":{"x":0,"y":0},"health":100}"#;
    // One-liner for a derived type
    let entity: Entity = nanojson::parse(&json)?;
    let entity: Entity = nanojson::parse(json.as_bytes())?;

    // Closure form for manual parsing
    let json = r#"{"x": 3, "y": 4}"#;
    let (x, y) = nanojson::parse_as(json.as_bytes(), |p| {
        p.object_begin()?;
        let mut x = 0i64; let mut y = 0i64;
        while let Some(k) = p.member()? {
            match k {
                "x" => x = p.integer()?,
                "y" => y = p.integer()?,
                _   => {}
            }
        }
        p.object_end()?;
        Ok((x, y))
    })?;

    // ------------------------------------------------
    // no_std tier
    // ------------------------------------------------

    // Serialization
    // One-liner for a derived type
    let mut buf = [0; 128];
    let json = nanojson::stringify_sized(&mut buf, &entity)?;

    // Closure form
    let json = nanojson::stringify_sized_as(&mut buf, |s| {
        s.object_begin()?;
          s.member("name")?; s.string("Alice")?;
          s.member("age")?;  s.integer(30)?;
        s.object_end()
    })?;

    // Deserialization
    let json = r#"{"id":42,"is_active":true,"position":{},"health":100}"#.as_bytes();
    // One-liner for a derived type (STR_BUF = 64)
    let entity: Entity = nanojson::parse_sized(&mut [0; 64], json)?;

    // Low-level parser for hand-written code
    let json = r#"{"x": 3, "y": 4}"#.as_bytes();
    let (x, y) = nanojson::parse_sized_as(&mut [0; 64], json, |p| {
        p.object_begin()?;
        let mut x = 0i64; let mut y = 0i64;
        while let Some(k) = p.member()? {
            match k {
                "x" => x = p.integer()?,
                "y" => y = p.integer()?,
                _   => {}
            }
        }
        p.object_end()?;
        Ok((x, y))
    })?;


    // Size estimation
    let n = nanojson::measure(|s| entity.serialize(s));
    // n is the exact byte count — use it to pick N in stringify_sized / parse_sized.


    // Derive macros
    // Add nanojson-derive as a dev-dependency and annotate your types:

    use nanojson::{Serialize, Deserialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Vec2 {
        #[nanojson(default)] // Allow member to be default initialized if
        x: i64,              // omitted during parsing.
        #[nanojson(default)]
        y: i64,
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Entity {
        id: i64,
        #[nanojson(rename = "is_active")]
        active: bool,
        position: Vec2,   // nested derived struct — works automatically
        health: i64,
    }

    // Unit enums serialize as JSON strings:
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    enum Team {
        Red,
        Blue,
        #[nanojson(rename = "spectator")]
        Spectator,
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    enum Event {
        // Struct-variant enums use externally-tagged format: {"VariantName": {...}}
        Spawn { entity_id: i64, x: i64, y: i64 },
        // Enum variants can be single value tuples (not multiple values)
        Death(i32),
    }

    // std tier roundtrip
    let entity = Entity { id: 42, active: true, position: Vec2 { x: 10, y: -5 }, health: 100 };

    let json: String = nanojson::stringify(&entity)?;
    // {"id":42,"is_active":true,"position":{"x":10,"y":-5},"health":100}

    let entity2: Entity = nanojson::parse(&json)?;
    assert_eq!(entity, entity2);

    // no_std tier roundtrip
    let mut buf = [0; 256];
    let json = nanojson::stringify_sized(&mut buf, &entity)?;

    let entity2: Entity = nanojson::parse_sized(&mut [0; 64], json)?;
    assert_eq!(entity, entity2);

    // Pretty-printing
    // Low-level — use Serializer::with_pretty directly:
    let mut buf = [0u8; 256];
    let mut w = SliceWriter::new(&mut buf);
    let mut ser: Serializer<_, 16> = Serializer::with_pretty(&mut w, 2); // 2-space indent

    Ok(())
}
