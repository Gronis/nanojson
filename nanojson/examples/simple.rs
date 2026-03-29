use nanojson::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct MyStruct {
    num: i32,
    name: String,
}

fn main() {
    // Parse a JSON string into a struct
    let json = r#"{"num": 42, "name": "hello"}"#;
    let my_struct = nanojson::parse::<MyStruct>(json);

    if let Ok(mut my_struct) = my_struct {
        println!("Parsed: {:?}", my_struct);

        // Change the fields and turn back into a JSON string again
        my_struct.num = 420;
        my_struct.name = "world".to_string();
        if let Ok(json) = nanojson::stringify(&my_struct) {
            println!("JSON: {}", json);
        }
    }
}
