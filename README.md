# nanojson

[![Crates.io](https://img.shields.io/crates/v/nanojson.svg)](https://crates.io/crates/nanojson)
[![Docs.rs](https://docs.rs/nanojson/badge.svg)](https://docs.rs/nanojson)
[![CI](https://github.com/Gronis/nanojson/actions/workflows/ci.yml/badge.svg)](https://github.com/Gronis/nanojson/actions)
[![License: MIT](https://img.shields.io/badge/license-MIT%20-blue.svg)](https://github.com/Gronis/nanojson/blob/main/LICENCE)
[![Rust 100% Safe](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/Gronis/nanojson/blob/main/nanojson/src/lib.rs)
[![no-std compatible](https://img.shields.io/badge/no%E2%80%93std-compatible-brightgreen)](https://github.com/Gronis/nanojson/blob/main/nanojson/src/lib.rs)


nanojson is a **zero-dependency**, `no-std` compatible JSON serializer and pull-parser with hand-written derives (no `serde`, no `pro-macro2`). It uses an immediate-mode API, validating your schema while parsing. 

```rust
use nanojson::{Serialize, Deserialize};

// Annotate your types once:
#[derive(Serialize, Deserialize)]
struct Point { x: i64, y: i64 }

// std API
let json: String = nanojson::stringify(&Point { x: 3, y: 4 })?;
let point: Point = nanojson::parse(&json)?;

// no-std API
let mut buf = [0; 256]; // <-- json string stored here
let json:  &str  = nanojson::stringify_sized(&mut buf, &Point { x: 3, y: 4 })?;
let point: Point = nanojson::parse_sized(&mut [0; 64], json)?; // <- provide scratch buffer
```

Provide blanket serialize and deserialize implementations for:
* `no-std`: primitive types, fixed sized arrays, slices
* optional support for:
  * `alloc`: `String`, `Vec<T>`, `Box<T>`, `BTreeMap<String, V>`
  * `std`: `HashMap<String, V>`
  * `arrayvec`: `ArrayVec<T, N>` and `ArrayString<N>`

---

## Derive macros

Use `derive` feature (enabled by default) and annotate your types:

```rust
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
    position: Vec2,   // nested derived struct, works automatically
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

// Struct-variant enums use externally-tagged format: {"VariantName": {...}}
#[derive(Serialize, Deserialize, Debug, PartialEq)]
enum Event {
    Spawn { entity_id: i64, x: i64, y: i64 },
    Death { entity_id: i64 },
}
```

---

## Feature tiers

| Feature | Enables |
|---|---|
| *(none)* | Core no_std/no_alloc tier: `parse_sized`, `stringify_sized`, fixed-size arrays, all primitives including `f32`/`f64` |
| `alloc` | `String`, `Vec<T>`, `Box<T>`, `BTreeMap<String, V>` |
| `std` *(default)* | Everything in `alloc` plus `HashMap<String, V>`, `stringify`/`parse` convenience functions |
| `derive` *(default)* | `#[derive(Serialize, Deserialize)]` macros |
| `arrayvec` | `ArrayVec<T, N>` and `ArrayString<N>` from the [`arrayvec`](https://docs.rs/arrayvec) crate |

---

## Two API tiers

### `std` tier (default feature)

No buffer choices. The output `String` grows as needed; the scratch buffer for parsing is auto-allocated to `src.len()` bytes (a safe upper bound).

#### Serialization

```rust
// One-liner for a derived type
let json: String = nanojson::stringify(&entity)?;

// Closure form for hand-written JSON
let json: String = nanojson::stringify_manual(|s| {
    s.object_begin()?;
      s.member_key("name")?; s.string("Alice")?;
      s.member_key("age")?;  s.integer(30)?;
    s.object_end()
})?;
```

#### Deserialization

```rust
// One-liner for a derived type
let entity: Entity = nanojson::parse(&json)?;

// Closure form for manual parsing
let json = r#"{"x": 3, "y": 4}"#;
let (x, y) = nanojson::parse_manual(json.as_bytes(), |p, buf| {
    p.object_begin()?;
    let mut x = 0i64; let mut y = 0i64;
    while let Some(k) = p.object_member(buf)? {
        match k {
            "x" => x = p.number_str()?.parse().unwrap(),
            "y" => y = p.number_str()?.parse().unwrap(),
            _   => {}
        }
    }
    p.object_end()?;
    Ok((x, y))
})?;
```

---

### `no_std` tier

All memory on the stack. You choose output buffer size when stringifying, and scratch buffer size for parsing (scratch buffer only needs to fit the longest single field value after escape-decoding, typically 32–128 bytes).

#### Serialization

```rust
// One-liner for a derived type
let mut buf = [0; 256];
let json = nanojson::stringify_sized(&mut buf, &entity)?;

// Closure form
let json = nanojson::stringify_manual_sized(&mut buf, |s| {
    s.object_begin()?;
      s.member_key("name")?; s.string("Alice")?;
      s.member_key("age")?;  s.integer(30)?;
    s.object_end()
})?;
```

#### Deserialization

```rust
// One-liner for a derived type (STR_BUF = 64)
let entity: Entity = nanojson::parse_sized(&mut [0; 64], &json)?;

// Low-level parser for hand-written code
let json = r#"{"x": 3, "y": 4}"#.as_bytes();
let (x, y) = nanojson::parse_manual_sized(&mut [0; 64], json, |p, buf| {
    p.object_begin()?;
    let mut x = 0i64; let mut y = 0i64;
    while let Some(k) = p.object_member(buf)? {
        match k {
            "x" => x = p.number_str()?.parse().unwrap(),
            "y" => y = p.number_str()?.parse().unwrap(),
            _   => {}
        }
    }
    p.object_end()?;
    Ok((x, y))
})?;
```

---

### Size estimation

```rust
let n = nanojson::measure(|s| entity.serialize(s));
// n is the exact byte count, use it to pick N in stringify_sized / parse_sized.
```

---

## Pretty-printing

Pass an indent width to the `_pretty` variants of any serialization function:

```rust
// std tier
let json = nanojson::stringify_pretty(2, &entity)?;
let json = nanojson::stringify_manual_pretty(2, |s| { ... })?;

// no_std tier
let mut buf = [0; 256];
let json = nanojson::stringify_sized_pretty(&mut buf, 2, &entity)?;
let json = nanojson::stringify_manual_sized_pretty(&mut buf, 2, |s| { ... })?;
```

```json
{
  "name": "Alice",
  "active": true,
  "level": 3
}
```

---

## Error handling

### Parse errors are `ParseError { kind: ParseErrorKind, offset: usize }`. 
The `offset` is a byte position in the source slice.

| Kind | Meaning |
|---|---|
| `UnexpectedToken { expected, got }` | Parser expected one token type, found another |
| `UnexpectedEof` | Input ended before the value was complete |
| `InvalidEscape(byte)` | Unknown `\X` escape sequence |
| `StringBufferOverflow` | Decoded string didn't fit in the scratch buffer |
| `InvalidUtf8` | String content is not valid UTF-8 after unescaping |
| `UnknownField { type_name, expected_fields }` | Key not recognised by the deserializer |
| `MissingField { field }` | A required `field` was absent (used by derived code) |

Parse error can be printed with a nice looking error message like so:

```rust
match nanojson::parse::<MyStruct>(json) {
    Err(err) =>  err.print(json),
    _        => { ... }
}
```
```txt
27 |    "not_a_valid_field": {},
   |    ^
   |    unknown field in `Inventory`, expected one of: `items`, `metadata`
```

### Serialization errors are `SerializeError<W::Error>`:

| Variant | Meaning |
|---|---|
| `Write(e)` | Write error from the sink (e.g. `WriteError::BufferFull` from `SliceWriter`) |
| `DepthExceeded` | Nesting exceeded the `DEPTH` const generic (default 32) |
| `InvalidState` | `member_key` called outside an object or twice without an intervening value |
| `InvalidUtf8(offset)` | Final string isn't utf-8 compatible. This indicates a serialization bug |

---

## Workspace layout

```
nanojson/
├── Cargo.toml                    workspace root
├── nanojson/                     core library (#![no_std], no dependencies)
│   ├── src/
│   │   ├── lib.rs
│   │   ├── write.rs              Write, SliceWriter, SizeCounter
│   │   ├── serialize.rs          Serializer, SerializeError, Serialize
│   │   ├── deserialize.rs        Parser, Deserialize
│   │   └── error.rs              Error types
│   ├── examples/
│   └── tests/
│       ├── readme.rs             tests for README.md code
│       ├── non_derive.rs         tests for manual parsing with derive trait
│       └── derive_roundtrip.rs   integration tests
└── nanojson-derive/              proc-macro crate (no syn/quote/proc_macro2)
```
---

## Core concepts

| Concept | What it is |
|---|---|
| `Serializer<W>` | Serializer. You call methods (`object_begin`, `member_key`, `integer`, …) in order and it writes JSON to your `W: Write` sink. |
| `Parser<'src, 'buf>` | Pull parser. You drive it step by step (`object_begin`, `object_member`, `number_str`, …). It never builds a tree. |
| `Write` | Trait for output sinks. `SliceWriter` writes into a `&mut [u8]`. `SizeCounter` counts bytes without writing (useful for pre-sizing). `Vec<u8>` implements `Write` when the `std` feature is on. |
| `Serialize` | Trait implemented by types that know how to write themselves. Primitive impls are provided. |
| `Deserialize` | Trait implemented by types that know how to parse themselves. |

---

## Limitations

- **Scratch buffer is reused per string.** `Parser::string()` and `Parser::object_member()` both write into the same `&mut [u8]`. The returned `&str` is invalidated by the next string parse. Copy the value immediately if you need to keep it.
- **No streaming / async.** The serializer writes synchronously to `Write`; the parser requires the entire input to be in memory at once.
- **No `serde` compatibility.** nanojson is its own trait ecosystem. If you need serde interop, use serde.
- **Non-finite floats are an error.** Serializing `f32::NAN`, `f64::INFINITY`, etc. returns `SerializeError::InvalidValue`. JSON has no representation for these values.
- **Nesting depth limit.** The serializer's `DEPTH` const generic (default 32) limits how deeply you can nest objects and arrays. Use `Serializer<W, 64>` directly for deeper structures.
- **No tuple enum variants** Use `enum Kind { A { number: i32 } }` instead of `enum Kind { A(i32) }`

---

## Running the examples

```sh
cargo run --example simple        # simple derive example
cargo run --example nostd         # almost same as simple but no_std
cargo run --example big           # big derive example
cargo run --example manual        # hand-written serialize + parse
cargo run --example derive        # derive-macro workflow
cargo run --example sensor_log    # embedded sensor log
cargo run --example recursive     # recursive tree + depth limits
```

## Running the tests

```sh
cargo test                    # std feature (default)
cargo test --no-default-features  # no_std mode
```
