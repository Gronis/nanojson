//! Derive-macro workflow — `Serialize` and `Deserialize`.
//!
//! Annotate your structs and enums and get serialize/deserialize for free.
//! Rename fields with `#[nanojson(rename = "...")]`.
//!
//! Shows both API tiers:
//!
//! **`std` tier**: `nanojson::to_string` / `nanojson::from_str` — one-liners,
//! no buffer choices.
//!
//! **`no_std` tier**: `nanojson::to_json::<N, _>` / `nanojson::from_json::<STR_BUF, _>` —
//! all memory on the stack, caller picks sizes.

extern crate std;

use nanojson::serialize::Serialize;
use nanojson_derive::{Serialize, Deserialize};

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

    let json = nanojson::to_string(&entity).unwrap();
    std::println!("Entity JSON (std): {json}");

    let entity2: Entity = nanojson::from_str(&json).unwrap();
    std::println!("Decoded (std):     {:?}", entity2);
    assert_eq!(entity, entity2);

    // ----------------------------------------------------------------
    // no_std tier — stack-allocated buffers, explicit sizes
    //
    // to_json::<N, _>  — N bytes for the output JSON.
    // from_json::<S, _> — S bytes for the string-decode scratch buffer;
    //                     only needs to fit the longest single field value.
    // ----------------------------------------------------------------

    let (buf, len) = nanojson::to_json::<256, _>(&entity).unwrap();
    std::println!("\nEntity JSON (no_std, {len} bytes): {}", core::str::from_utf8(&buf[..len]).unwrap());

    let entity3: Entity = nanojson::from_json::<64, _>(&buf[..len]).unwrap();
    std::println!("Decoded (no_std):  {:?}", entity3);
    assert_eq!(entity, entity3);

    // ----------------------------------------------------------------
    // Unit enum — std and no_std
    // ----------------------------------------------------------------

    let team = Team::Spectator;

    let json = nanojson::to_string(&team).unwrap();
    std::println!("\nTeam JSON (std):    {json}");
    let team2: Team = nanojson::from_str(&json).unwrap();
    assert_eq!(team, team2);

    let (buf, len) = nanojson::to_json::<16, _>(&team).unwrap();
    std::println!("Team JSON (no_std): {}", core::str::from_utf8(&buf[..len]).unwrap());
    let team3: Team = nanojson::from_json::<16, _>(&buf[..len]).unwrap();
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
        let json = nanojson::to_string(ev).unwrap();
        std::println!("\nEvent JSON (std):    {json}");
        let ev2: Event = nanojson::from_str(&json).unwrap();
        assert_eq!(*ev, ev2);

        // no_std
        let (buf, len) = nanojson::to_json::<128, _>(ev).unwrap();
        std::println!("Event JSON (no_std): {}", core::str::from_utf8(&buf[..len]).unwrap());
        let ev3: Event = nanojson::from_json::<32, _>(&buf[..len]).unwrap();
        assert_eq!(*ev, ev3);
    }

    // ----------------------------------------------------------------
    // Error handling — unknown field, missing field
    // ----------------------------------------------------------------

    let bad = r#"{"id":1,"is_active":true,"position":{"x":0,"y":0},"health":100,"unknown":999}"#;
    match nanojson::from_str::<Entity>(bad) {
        Err(e) => std::println!("\nExpected error (unknown field): {:?} at offset {}", e.kind, e.offset),
        Ok(_)  => panic!("should have failed"),
    }

    let incomplete = r#"{"id":2,"is_active":false,"position":{"x":1,"y":2}}"#;
    match nanojson::from_str::<Entity>(incomplete) {
        Err(e) => std::println!("Expected error (missing field): {:?} at offset {}", e.kind, e.offset),
        Ok(_)  => panic!("should have failed"),
    }

    // ----------------------------------------------------------------
    // Size estimation — measure before picking N
    // ----------------------------------------------------------------

    let n = nanojson::measure(|s| entity.serialize(s));
    std::println!("\nEntity serializes to {n} bytes — use to_json::<{n}, _> or larger.");
}
