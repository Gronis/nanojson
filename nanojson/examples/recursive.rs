//! Recursive JSON tree example.
//!
//! Shows how to work with self-referential data structures: a tree where each
//! node can contain an arbitrary number of child nodes. This requires heap
//! allocation, so the example uses the `std` feature.
//!
//! Key points demonstrated:
//!
//! - Defining a recursive type and implementing `Serialize` for it.
//! - Serializing a shallow tree (succeeds) vs a deep tree (`DepthExceeded`).
//! - Raising the depth limit by writing a free function generic over `DEPTH`.
//! - A depth-limited recursive parser that returns a clear error instead of
//!   stack-overflowing on adversarial input.

extern crate std;

use nanojson::{Serialize, Serializer, SerializeError, Write, Parser};

// ---- Domain type --------------------------------------------------------

/// A minimal recursive JSON tree.
///
/// `Array` wraps a heap-allocated `Vec` so the type can refer to itself.
#[derive(Debug, PartialEq)]
enum Tree {
    Int(i64),
    Array(std::vec::Vec<Tree>),
}

impl Tree {
    /// Build `[[[...[n]...]]]` `depth` levels deep.
    fn nested(depth: usize, n: i64) -> Self {
        let mut node = Tree::Int(n);
        for _ in 0..depth {
            node = Tree::Array(std::vec![node]);
        }
        node
    }

    /// Count the total number of `Int` leaves.
    fn leaf_count(&self) -> usize {
        match self {
            Tree::Int(_) => 1,
            Tree::Array(children) => children.iter().map(|c| c.leaf_count()).sum(),
        }
    }
}

// ---- Serialize impl -------------------------------------------------------
//
// The `Serialize` trait signature is `fn serialize<W: Write>(&self, ser: &mut
// Serializer<W>)` — which uses the default DEPTH of 32. To support arbitrary
// depths, the real logic lives in the generic free function `write_tree<W,
// DEPTH>` below. The trait impl just calls it; and for deeper trees the caller
// can call `write_tree` directly with a higher DEPTH.

fn write_tree<W: Write, const DEPTH: usize>(
    tree: &Tree,
    s: &mut Serializer<W, DEPTH>,
) -> Result<(), SerializeError<W::Error>> {
    match tree {
        Tree::Int(n) => s.integer(*n),
        Tree::Array(children) => {
            s.array_begin()?;
            for child in children {
                write_tree(child, s)?;
            }
            s.array_end()
        }
    }
}

impl Serialize for Tree {
    fn serialize<W: Write>(
        &self,
        s: &mut Serializer<W>,
    ) -> Result<(), SerializeError<W::Error>> {
        write_tree(self, s)
    }
}

// ---- Depth-limited parser -------------------------------------------------

/// Error returned by the depth-limited recursive parser.
#[derive(Debug)]
enum ParseTreeError {
    /// A structural JSON parse error from the underlying parser.
    Json(nanojson::ParseError),
    /// Input is nested more deeply than the caller-supplied limit.
    NestingTooDeep { limit: usize },
}

impl From<nanojson::ParseError> for ParseTreeError {
    fn from(e: nanojson::ParseError) -> Self {
        ParseTreeError::Json(e)
    }
}

/// Parse a `Tree` from `parser`, failing if nesting exceeds `max_depth`.
///
/// Pass `current_depth = 0` at the call site.
///
/// Without this guard, a deeply-nested input like `[[[[...]]]]` would drive
/// Rust's call stack until it overflows — undefined behavior on bare-metal,
/// a crash with std. Tracking depth here keeps failure predictable.
fn parse_tree(
    parser: &mut Parser,
    current_depth: usize,
    max_depth: usize,
) -> Result<Tree, ParseTreeError> {
    if current_depth > max_depth {
        return Err(ParseTreeError::NestingTooDeep { limit: max_depth });
    }

    if parser.is_array_ahead() {
        parser.array_begin()?;
        let mut children = std::vec::Vec::new();
        while parser.array_item()? {
            children.push(parse_tree(parser, current_depth + 1, max_depth)?);
        }
        parser.array_end()?;
        Ok(Tree::Array(children))
    } else {
        let n: i64 = parser.number_str()?.parse().unwrap_or(0);
        Ok(Tree::Int(n))
    }
}

// ---- main -----------------------------------------------------------------

fn main() {
    // ------------------------------------------------------------------
    // 1. Shallow tree — works with the default depth limit (32).
    // ------------------------------------------------------------------
    let shallow = Tree::nested(5, 42);
    let json = nanojson::stringify(&shallow).unwrap();
    std::println!("Shallow tree (5 deep): {json}");

    // ------------------------------------------------------------------
    // 2. Deep tree — exceeds the default Serializer depth of 32.
    //
    //    `stringify` calls `val.serialize(&mut ser)` via the `Serialize`
    //    trait, which uses `Serializer<Vec<u8>, 32>`. Any structure more
    //    than 32 levels deep hits the stack limit and returns DepthExceeded.
    // ------------------------------------------------------------------
    let deep = Tree::nested(50, 7);

    match nanojson::stringify(&deep) {
        Err(SerializeError::DepthExceeded) => {
            std::println!("\nDeep tree (50 levels) via `stringify`: DepthExceeded.");
            std::println!("  The `Serialize` trait uses Serializer<W, 32> (the default).");
        }
        Ok(json) => std::println!("  Unexpectedly succeeded: {} bytes", json.len()),
        Err(e) => std::println!("  Unexpected error: {e:?}"),
    }

    // ------------------------------------------------------------------
    // 3. Raise the limit with `write_tree` directly.
    //
    //    `write_tree` is generic over DEPTH. By creating
    //    `Serializer<Vec<u8>, 64>` ourselves and calling `write_tree`
    //    rather than going through the `Serialize` trait, we get 64
    //    nesting levels.
    // ------------------------------------------------------------------
    {
        let mut ser: Serializer<std::vec::Vec<u8>, 64> =
            Serializer::new(std::vec::Vec::new());
        write_tree(&deep, &mut ser).unwrap();
        let vec = ser.into_writer();
        // SAFETY: the serializer only writes ASCII JSON tokens (all valid UTF-8).
        let json = std::string::String::from_utf8(vec).unwrap();
        std::println!("\nDeep tree (50 levels) with DEPTH=64: {} bytes serialized.", json.len());
    }

    // Even a 200-level deep tree fits with DEPTH=256:
    let very_deep = Tree::nested(200, 1);
    {
        let mut ser: Serializer<std::vec::Vec<u8>, 256> =
            Serializer::new(std::vec::Vec::new());
        match write_tree(&very_deep, &mut ser) {
            Ok(()) => {
                let len = ser.into_writer().len();
                std::println!("Very deep tree (200 levels) with DEPTH=256: {len} bytes.");
            }
            Err(SerializeError::DepthExceeded) => std::println!("Still too deep."),
            Err(e) => std::println!("Error: {e:?}"),
        }
    }

    // ------------------------------------------------------------------
    // 4. Parsing with a depth limit.
    //
    //    The parser has no built-in depth counter — it's a pull parser
    //    that you drive from your own (potentially recursive) code.
    //    A naive recursive function without a depth guard would
    //    stack-overflow on input like [[[[...10000 levels...]]]] because
    //    each `[` causes a recursive Rust call.
    //
    //    `parse_tree` above tracks depth and returns `NestingTooDeep`
    //    before the stack overflows.
    // ------------------------------------------------------------------

    let src_10 = nanojson::stringify(&Tree::nested(10, 99)).unwrap();
    std::println!("\nSource JSON (10 deep, first 40 chars): {:.40}...", &src_10);

    // 4a. Generous limit — succeeds.
    let mut parser = Parser::new(src_10.as_bytes());
    match parse_tree(&mut parser, 0, 20) {
        Ok(tree) => std::println!(
            "Parsed (limit=20, actual depth=10): {} leaf/leaves found.",
            tree.leaf_count()
        ),
        Err(e) => std::println!("Error: {e:?}"),
    }

    // 4b. Tight limit — fails with NestingTooDeep.
    let mut parser = Parser::new(src_10.as_bytes());
    match parse_tree(&mut parser, 0, 5) {
        Ok(_) => std::println!("Unexpectedly parsed."),
        Err(ParseTreeError::NestingTooDeep { limit }) => {
            std::println!("Parse failed: NestingTooDeep (limit={limit}, actual depth=10).");
        }
        Err(ParseTreeError::Json(e)) => std::println!("JSON error: {e:?}"),
    }

    // ------------------------------------------------------------------
    // 5. Round-trip: a tree that fits in both the default serializer
    //    depth (32) and the parse limit.
    // ------------------------------------------------------------------
    let fitting = Tree::nested(30, 5);
    let json = nanojson::stringify(&fitting).unwrap();
    let mut parser = Parser::new(json.as_bytes());
    let parsed = parse_tree(&mut parser, 0, 32).unwrap();
    assert_eq!(parsed.leaf_count(), 1);
    std::println!("\nRound-trip (30 deep, default limits): OK.");
}
