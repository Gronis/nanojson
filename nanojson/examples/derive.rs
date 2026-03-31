//! Derive-macro workflow — `Serialize` and `Deserialize`.
//!
//! Annotate your structs and enums and get serialize/deserialize for free.
//! Rename fields with `#[nanojson(rename = "...")]`.
//!
//! Shows both API tiers:
//!
//! **`std` tier**: `nanojson::stringify` / `nanojson::parse` — one-liners,
//! no buffer choices.
//!
//! **`no_std` tier**: `nanojson::stringify_sized::<N, _>` / `nanojson::parse_sized::<STR_BUF, _>` —
//! all memory on the stack, caller picks sizes.

extern crate std;

use nanojson::{Serialize, Deserialize};

// ---- Domain types ----

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Vec2 {
    x: i64,
    y: i64,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Entity {
    id: i64,
    #[nanojson(rename = "is_active")]
    active: bool,
    position: Vec2,
    health: i64,
}

// Unit enums serialize to/from JSON strings.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
enum Team {
    Red,
    Blue,
    #[nanojson(rename = "spectator")]
    Spectator,
}

// Enums with data use externally-tagged format: {"VariantName": {...}}.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
enum Event {
    Spawn { entity_id: i64, x: i64, y: i64 },
    Death { entity_id: i64 },
}

fn main() {
    let entity = Entity {
        id: 42,
        active: true,
        position: Vec2 { x: 100, y: -50 },
        health: 80,
    };

    // ----------------------------------------------------------------
    // std tier — String in, String out, no buffer choices
    // ----------------------------------------------------------------

    let json = nanojson::stringify(&entity).unwrap();
    std::println!("Entity JSON (std): {json}");

    let entity2: Entity = nanojson::parse(&json).unwrap();
    std::println!("Decoded (std):     {:?}", entity2);
    assert_eq!(entity, entity2);

    // ----------------------------------------------------------------
    // no_std tier — stack-allocated buffers, explicit sizes
    //
    // stringify_sized::<N, _>  — N bytes for the output JSON.
    // parse_sized::<S, _> — S bytes for the string-decode scratch buffer;
    //                     only needs to fit the longest single field value.
    // ----------------------------------------------------------------

    let mut buf = [0; 256];
    let json = nanojson::stringify_sized(&mut buf, &entity).unwrap();
    std::println!("\nEntity JSON (no_std, {} bytes): {}", json.len(), json);

    let entity3: Entity = nanojson::parse_sized(&mut [0; 64], json).unwrap();
    std::println!("Decoded (no_std):  {:?}", entity3);
    assert_eq!(entity, entity3);

    // ----------------------------------------------------------------
    // Unit enum — std and no_std
    // ----------------------------------------------------------------

    let team = Team::Spectator;

    let json = nanojson::stringify(&team).unwrap();
    std::println!("\nTeam JSON (std):    {json}");
    let team2: Team = nanojson::parse(&json).unwrap();
    assert_eq!(team, team2);

    let mut buf = [0; 16];
    let json = nanojson::stringify_sized(&mut buf, &team).unwrap();
    std::println!("Team JSON (no_std): {}", json);
    let team3: Team = nanojson::parse_sized(&mut [0; 16], json).unwrap();
    assert_eq!(team, team3);

    // ----------------------------------------------------------------
    // Data enum
    // ----------------------------------------------------------------

    let events: [Event; 2] = [
        Event::Spawn { entity_id: 1, x: 0, y: 0 },
        Event::Death { entity_id: 1 },
    ];

    for ev in &events {
        // std
        let json = nanojson::stringify(ev).unwrap();
        std::println!("\nEvent JSON (std):    {json}");
        let ev2: Event = nanojson::parse(&json).unwrap();
        assert_eq!(*ev, ev2);

        // no_std
        let mut buf = [0; 128];
        let json = nanojson::stringify_sized(&mut buf, ev).unwrap();
        std::println!("Event JSON (no_std): {}", json);
        let ev3: Event = nanojson::parse_sized(&mut [0; 32], json).unwrap();
        assert_eq!(*ev, ev3);
    }

    // ----------------------------------------------------------------
    // Error handling — unknown field, missing field
    // ----------------------------------------------------------------

    let bad = r#"{"id":1,"is_active":true,"position":{"x":0,"y":0},"health":100,"unknown":999}"#;
    match nanojson::parse::<Entity>(bad) {
        Err(e) => std::println!("\nExpected error (unknown field): {:?} at offset {}", e.kind, e.offset),
        Ok(_)  => panic!("should have failed"),
    }

    let incomplete = r#"{"id":2,"is_active":false,"position":{"x":1,"y":2}}"#;
    match nanojson::parse::<Entity>(incomplete) {
        Err(e) => std::println!("Expected error (missing field): {:?} at offset {}", e.kind, e.offset),
        Ok(_)  => panic!("should have failed"),
    }

    // ----------------------------------------------------------------
    // Size estimation — measure before picking N
    // ----------------------------------------------------------------

    let n = nanojson::measure(|s| entity.serialize(s));
    std::println!("\nEntity serializes to {n} bytes — use stringify_sized::<{n}, _> or larger.");
}

#[cfg(test)] #[test] fn test_main() { main() }
