extern crate std;

use std::collections::HashMap;
use nanojson::{Serialize, Deserialize};

// -----------------------------
// Core nested types
// -----------------------------

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Address {
    street: String,
    city: String,
    zip: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Profile {
    username: String,
    email: String,
    address: Address,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Item {
    id: i64,
    name: String,
    price: i64,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Inventory {
    items: Vec<Item>,
    metadata: HashMap<String, String>,
}

// -----------------------------
// Enums (mixed usage)
// -----------------------------

#[derive(Serialize, Deserialize, Debug, PartialEq)]
enum Role {
    Admin,
    User,
    Guest,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
enum Status {
    Active,
    Disabled,
    Pending,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
enum Event {
    Login { user_id: i64 },
    Purchase { item_id: i64, quantity: i64 },
    Logout,
}

// -----------------------------
// Big root structure
// -----------------------------

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct User {
    id: i64,
    profile: Box<Profile>,              // Box<T>
    roles: Vec<Role>,                   // Vec<T>
    status: Status,
    tags: Vec<String>,                  // Vec<String>
    preferences: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct AppState {
    users: Vec<User>,
    inventory: Inventory,
    sessions: HashMap<String, i32>,
    events: Vec<Event>,
}

// -----------------------------
// Test
// -----------------------------

fn main() {
    let mut prefs = HashMap::new();
    prefs.insert("theme".to_string(), "dark".to_string());
    prefs.insert("language".to_string(), "en".to_string());

    let mut metadata = HashMap::new();
    metadata.insert("warehouse".to_string(), "A1".to_string());
    metadata.insert("currency".to_string(), "USD".to_string());

    let mut sessions = HashMap::new();
    sessions.insert("session_abc".to_string(), 1);
    sessions.insert("session_xyz".to_string(), 2);

    let state = AppState {
        users: vec![
            User {
                id: 1,
                profile: Box::new(Profile {
                    username: "alice".to_string(),
                    email: "alice@example.com".to_string(),
                    address: Address {
                        street: "Main St".to_string(),
                        city: "Wonderland".to_string(),
                        zip: "12345".to_string(),
                    },
                }),
                roles: vec![Role::Admin, Role::User],
                status: Status::Active,
                tags: vec!["premium".to_string(), "beta".to_string()],
                preferences: prefs.clone(),
            },
            User {
                id: 2,
                profile: Box::new(Profile {
                    username: "bob".to_string(),
                    email: "bob@example.com".to_string(),
                    address: Address {
                        street: "Second St".to_string(),
                        city: "Builderland".to_string(),
                        zip: "67890".to_string(),
                    },
                }),
                roles: vec![Role::User],
                status: Status::Pending,
                tags: vec![],
                preferences: prefs.clone(),
            },
        ],
        inventory: Inventory {
            items: vec![
                Item { id: 1, name: "Sword".to_string(), price: 100 },
                Item { id: 2, name: "Shield".to_string(), price: 150 },
            ],
            metadata,
        },
        sessions,
        events: vec![
            Event::Login { user_id: 1 },
            Event::Purchase { item_id: 2, quantity: 1 },
            Event::Logout,
        ],
    };

    // ------------------------------------------------------------
    // Serialize
    // ------------------------------------------------------------

    let json = nanojson::stringify_pretty(&state, 2).unwrap();
    std::println!("AppState JSON:\n{json}");

    // ------------------------------------------------------------
    // Deserialize
    // ------------------------------------------------------------

    let decoded: AppState = nanojson::parse(&json).unwrap();
    std::println!("\nDecoded:\n{:?}", decoded);

    assert_eq!(state, decoded);

    // ------------------------------------------------------------
    // Manual long JSON test (hardcoded)
    // ------------------------------------------------------------

    let raw = r#"
    {
        "users": [
            {
                "id": 3,
                "profile": {
                    "username": "charlie",
                    "email": "charlie@example.com",
                    "address": {
                        "street": "Third St",
                        "city": "ChocolateFactory",
                        "zip": "99999"
                    }
                },
                "roles": ["Guest"],
                "status": "Disabled",
                "tags": ["new"],
                "preferences": {
                    "theme": "light"
                }
            }
        ],
        "inventory": {
            "items": [
                { "id": 10, "name": "Potion", "price": 50 }
            ],
            "metadata": {
                "warehouse": "B2"
            }
        },
        "sessions": {
            "session_foo": 3
        },
        "events": [
            { "Login": { "user_id": 3 } },
            { "Logout": null }
        ]
    }
    "#;

    println!("");
    match nanojson::parse::<AppState>(raw) {
        Ok(parsed) => {
            std::println!("\nParsed from raw JSON:\n{:?}", parsed);
        }
        Err(e) =>  e.print(raw),
    }
}
