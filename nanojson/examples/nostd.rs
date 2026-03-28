#![no_std]

use nanojson::{Deserialize, ParseError, ParseErrorKind, Parser, Serialize, SerializeError, Serializer, Write};

struct StringBuf<const N: usize> {
    buf: [u8; N],
    len: usize,
}

#[derive(Serialize, Deserialize, Debug)]
struct MyStruct {
    num: i32,
    name: StringBuf<32>,
}

fn main() {
    // Parse a JSON string into a struct
    let json = r#"{"num": 42, "name": "hello"}"#;
    let my_struct = nanojson::from_json::<256, MyStruct>(json.as_bytes());

    if let Ok(my_struct) = my_struct {
        // Change the fields and turn back into a JSON string again
        let my_struct2 = MyStruct {
            num: 420,
            name: StringBuf::try_from("world").unwrap(),
        };

        if let Ok(json) = nanojson::to_string(&my_struct2) {
            // Use panic! to print the parsed struct and the JSON string since
            // we are in no_std land
            panic!("Parsed: {my_struct:?}\nJSON: {json}");
        }
    }
}

impl<const N: usize> TryFrom<&str> for StringBuf<N> {
    type Error = &'static str;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let mut buf = [0; N];
        if s.len() >= N {
            return Err("String too large")
        }
        buf[..s.len()].copy_from_slice(s.as_bytes());
        Ok(StringBuf { buf, len: s.len() })
    }
}

impl<const N: usize> core::fmt::Debug for StringBuf<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(core::str::from_utf8(&self.buf[..self.len]).unwrap())
    }
}

impl<const N: usize> Serialize for StringBuf<N> {
    fn serialize<W: Write>(&self, json: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        json.string_bytes(&self.buf[..self.len])
    }
}

impl<'src, 'buf, const N: usize> Deserialize<'src, 'buf> for StringBuf<N> {
    fn deserialize(json: &mut Parser<'src, 'buf>) -> Result<Self, ParseError> {
        let s = json.string()?;
        if s.len() > N {
            let offset = json.error_offset() + N - s.len();
            return Err(ParseError { kind: ParseErrorKind::StringBufferOverflow, offset });
        }
        let mut buf = [0; N];
        buf[..s.len()].copy_from_slice(s.as_bytes());
        Ok(StringBuf { buf, len: s.len() })
    }
}
