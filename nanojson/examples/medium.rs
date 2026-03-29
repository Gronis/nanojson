extern crate std;

use nanojson::{Serialize, Deserialize};

// -----------------------------
// Deep nested structures
// -----------------------------

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Comment {
    user: String,
    message: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Post {
    id: i64,
    title: String,
    body: String,
    comments: Vec<Comment>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Thread {
    name: String,
    posts: Vec<Post>,
}

// -----------------------------
// Tree using Box<T>
// -----------------------------

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Node {
    value: String,
    children: Vec<Box<Node>>, // recursive + Box
}

// -----------------------------
// Enums
// -----------------------------

#[derive(Serialize, Deserialize, Debug, PartialEq)]
enum Reaction {
    Like,
    Dislike,
    Laugh,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
enum Activity {
    View { post_id: i64 },
    React { post_id: i64, reaction: Reaction },
    Comment { post_id: i64, message: String },
}

// -----------------------------
// Big root
// -----------------------------

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Forum {
    threads: Vec<Thread>,
    activity_feed: Vec<Activity>,
    category_tree: Box<Node>,
    tags: Vec<Vec<String>>, // nested Vec stress
}

// -----------------------------
// Test
// -----------------------------

fn main() {
    let forum = Forum {
        threads: vec![
            Thread {
                name: "General".to_string(),
                posts: vec![
                    Post {
                        id: 1,
                        title: "Welcome".to_string(),
                        body: "Welcome to the forum!".to_string(),
                        comments: vec![
                            Comment {
                                user: "alice".to_string(),
                                message: "Nice to be here!".to_string(),
                            },
                            Comment {
                                user: "bob".to_string(),
                                message: "Hello everyone!".to_string(),
                            },
                        ],
                    },
                    Post {
                        id: 2,
                        title: "Rules".to_string(),
                        body: "Be nice.".to_string(),
                        comments: vec![],
                    },
                ],
            },
            Thread {
                name: "Rust".to_string(),
                posts: vec![
                    Post {
                        id: 3,
                        title: "Ownership".to_string(),
                        body: "Let's discuss ownership.".to_string(),
                        comments: vec![
                            Comment {
                                user: "charlie".to_string(),
                                message: "It's tricky at first.".to_string(),
                            }
                        ],
                    }
                ],
            }
        ],

        activity_feed: vec![
            Activity::View { post_id: 1 },
            Activity::React { post_id: 1, reaction: Reaction::Like },
            Activity::Comment { post_id: 2, message: "Important!".to_string() },
        ],

        category_tree: Box::new(Node {
            value: "root".to_string(),
            children: vec![
                Box::new(Node {
                    value: "programming".to_string(),
                    children: vec![
                        Box::new(Node {
                            value: "rust".to_string(),
                            children: vec![],
                        }),
                        Box::new(Node {
                            value: "c++".to_string(),
                            children: vec![],
                        }),
                    ],
                }),
                Box::new(Node {
                    value: "offtopic".to_string(),
                    children: vec![],
                }),
            ],
        }),

        tags: vec![
            vec!["rust".to_string(), "systems".to_string()],
            vec!["fun".to_string()],
            vec![],
        ],
    };

    // ------------------------------------------------------------
    // Serialize
    // ------------------------------------------------------------

    let json = nanojson::stringify_pretty(&forum, 2).unwrap();
    std::println!("Forum JSON:\n{json}");

    // ------------------------------------------------------------
    // Deserialize
    // ------------------------------------------------------------

    let decoded: Forum = nanojson::parse(&json).unwrap();
    std::println!("\nDecoded:\n{:?}", decoded);

    assert_eq!(forum, decoded);

    // ------------------------------------------------------------
    // Hardcoded JSON test
    // ------------------------------------------------------------

    let raw = r#"
    {
        "threads": [
            {
                "name": "Announcements",
                "posts": [
                    {
                        "id": 10,
                        "title": "Update",
                        "body": "New features added",
                        "comments": [
                            { "user": "admin", "message": "Enjoy!" }
                        ]
                    }
                ]
            }
        ],
        "activity_feed": [
            { "View": { "post_id": 10 } },
            { "React": { "post_id": 10, "reaction": "Laugh" } }
        ],
        "category_tree": {
            "value": "root",
            "children": []
        },
        "tags": [
            ["news"],
            []
        ]
    }
    "#;

    println!("");
    match nanojson::parse::<Forum>(raw) {
        Ok(parsed) => {
            std::println!("\nParsed from raw JSON:\n{:?}", parsed);
        }
        Err(e) =>  e.print(raw),
    }
}
