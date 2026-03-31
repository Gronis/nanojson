//! Manual serialization and deserialization — no derive macros.
//!
//! Shows both tiers of the API:
//!
//! **`std` tier** (`default`): `nanojson::stringify` / `nanojson::parse_as` —
//! no buffer choices, heap grows as needed.
//!
//! **`no_std` tier**: `nanojson::serialize::<N>` / `nanojson::parse::<STR_BUF>` —
//! all memory on the stack, caller picks sizes.
//!
//! The low-level `Serializer` / `Parser` / `SliceWriter` primitives are still
//! used directly in the final section to show pretty-printing, which has no
//! convenience wrapper yet.

extern crate std;

use nanojson::{Serializer, Parser, SliceWriter};

fn main() {
    // ----------------------------------------------------------------
    // 1. std tier — serialize into a String, no size choice needed
    // ----------------------------------------------------------------

    let json = nanojson::stringify_as(|s| {
        s.object_begin()?;
          s.member_key("name")?;   s.string("Alice")?;
          s.member_key("scores")?;
            s.array_begin()?;
              s.integer(95)?;
              s.integer(87)?;
              s.integer(100)?;
            s.array_end()?;
          s.member_key("meta")?;
            s.object_begin()?;
              s.member_key("active")?; s.boolean(true)?;
              s.member_key("level")?;  s.integer(3)?;
            s.object_end()?;
        s.object_end()
    })
    .unwrap();

    std::println!("Serialized (std): {json}");

    // ----------------------------------------------------------------
    // 2. std tier — parse via closure, scratch buffer auto-allocated
    // ----------------------------------------------------------------

    let mut name_bytes = [0u8; 32];
    let mut name_len = 0usize;
    let mut scores = [0i64; 8];
    let mut score_count = 0usize;
    let mut active = false;
    let mut level = 0i64;

    nanojson::parse_as(json.as_bytes(), |p| {
        p.object_begin()?;
        while let Some(key) = p.object_member()? {
            // key is &'src str — borrows the source, no copy needed.
            match key {
                "name" => {
                    let s = p.string()?;
                    name_len = s.len();
                    name_bytes[..name_len].copy_from_slice(s.as_bytes());
                }
                "scores" => {
                    p.array_begin()?;
                    while p.array_item()? {
                        scores[score_count] = p.integer()?;
                        score_count += 1;
                    }
                    p.array_end()?;
                }
                "meta" => {
                    p.object_begin()?;
                    while let Some(mk) = p.object_member()? {
                        match mk {
                            "active" => { active = p.boolean()?; }
                            "level"  => { level  = p.integer()?; }
                            other    => panic!("unknown meta field: {other}"),
                        }
                    }
                    p.object_end()?;
                }
                other => panic!("unknown field: {other}"),
            }
        }
        p.object_end()?;
        Ok(())
    })
    .unwrap();

    let name = core::str::from_utf8(&name_bytes[..name_len]).unwrap();
    std::println!("name:    {name}");
    std::println!("scores:  {:?}", &scores[..score_count]);
    std::println!("active:  {active}");
    std::println!("level:   {level}");

    // ----------------------------------------------------------------
    // 3. no_std tier — serialize into a fixed [u8; 512] stack buffer
    // ----------------------------------------------------------------

    let mut buf = [0; 512];
    let json = nanojson::stringify_sized_as(&mut buf, |s| {
        s.object_begin()?;
          s.member_key("name")?;   s.string("Alice")?;
          s.member_key("scores")?;
            s.array_begin()?;
              s.integer(95)?;
              s.integer(87)?;
              s.integer(100)?;
            s.array_end()?;
          s.member_key("meta")?;
            s.object_begin()?;
              s.member_key("active")?; s.boolean(true)?;
              s.member_key("level")?;  s.integer(3)?;
            s.object_end()?;
        s.object_end()
    })
    .unwrap();

    std::println!("\nSerialized (no_std, {} bytes): {}", json.len(), json);

    // ----------------------------------------------------------------
    // 4. no_std tier — parse with a 64-byte stack scratch buffer
    //
    //    The scratch buffer only needs to fit the longest single string
    //    value after escape-decoding; 64 bytes is ample here.
    //    Object keys borrow from the source directly — no buffer needed.
    // ----------------------------------------------------------------

    let mut name_bytes = [0u8; 32];
    let mut name_len = 0usize;
    let mut scores = [0i64; 8];
    let mut score_count = 0usize;
    let mut active = false;
    let mut level = 0i64;

    let mut str_buf = [0u8; 64];
    let mut p = Parser::new(json.as_bytes(), &mut str_buf);

    p.object_begin().unwrap();
    while let Some(key) = p.object_member().unwrap() {
        match key {
            "name" => {
                let s = p.string().unwrap();
                name_len = s.len();
                name_bytes[..name_len].copy_from_slice(s.as_bytes());
            }
            "scores" => {
                p.array_begin().unwrap();
                while p.array_item().unwrap() {
                    scores[score_count] = p.integer().unwrap();
                    score_count += 1;
                }
                p.array_end().unwrap();
            }
            "meta" => {
                p.object_begin().unwrap();
                while let Some(mk) = p.object_member().unwrap() {
                    match mk {
                        "active" => { active = p.boolean().unwrap(); }
                        "level"  => { level  = p.integer().unwrap(); }
                        other    => panic!("unknown meta field: {other}"),
                    }
                }
                p.object_end().unwrap();
            }
            other => panic!("unknown field: {other}"),
        }
    }
    p.object_end().unwrap();

    let name = core::str::from_utf8(&name_bytes[..name_len]).unwrap();
    std::println!("\nParsed (no_std):");
    std::println!("name:    {name}");
    std::println!("scores:  {:?}", &scores[..score_count]);
    std::println!("active:  {active}");
    std::println!("level:   {level}");

    // ----------------------------------------------------------------
    // 5. Pretty-printing — uses Serializer directly (no convenience
    //    wrapper for this yet; SliceWriter is fine for a fixed output)
    // ----------------------------------------------------------------

    let mut pretty_buf = [0u8; 512];
    let pretty_len;
    {
        let mut w = SliceWriter::new(&mut pretty_buf);
        let mut ser: Serializer<_, 16> = Serializer::with_pp(&mut w, 2);

        ser.object_begin().unwrap();
          ser.member_key("name").unwrap();   ser.string(name).unwrap();
          ser.member_key("active").unwrap(); ser.boolean(active).unwrap();
          ser.member_key("level").unwrap();  ser.integer(level).unwrap();
        ser.object_end().unwrap();

        pretty_len = w.pos();
    }

    std::println!("\nPretty-printed:\n{}", core::str::from_utf8(&pretty_buf[..pretty_len]).unwrap());

    // ----------------------------------------------------------------
    // 6. Size estimation — measure before committing to a buffer size
    // ----------------------------------------------------------------

    let n = nanojson::measure(|s| {
        s.object_begin()?;
          s.member_key("name")?;  s.string("Alice")?;
          s.member_key("level")?; s.integer(3)?;
        s.object_end()
    });
    std::println!("\n`{{\"name\":\"Alice\",\"level\":3}}` is {n} bytes.");
    // Use the measured size to pick N: serialize::<{n}>(...) or similar.
}

#[cfg(test)] #[test] fn test_main() { main() }
