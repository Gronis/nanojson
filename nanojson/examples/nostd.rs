#![no_std]

use nanojson::{Deserialize, ParseError, ParseErrorKind, Parser, Serialize, SerializeError, Serializer, Write};

struct StringArray<const N: usize> {
    buf: [u8; N],
    len: usize,
}

#[derive(Serialize, Deserialize, Debug)]
struct MyStruct {
    num: f32,
    name: StringArray<32>,
}

fn main() {
    // Parse a JSON string into a struct
    let json = r#"{"num": 42.3, "name": "hello"}"#;
    let my_struct = nanojson::parse_sized::<256, MyStruct>(json.as_bytes());

    if let Ok(my_struct) = my_struct {
        // Change the fields and turn back into a JSON string again
        let my_struct2 = MyStruct {
            num: 420.3,
            name: StringArray::try_from("world").unwrap(),
        };

        if let Ok((buff, len)) = nanojson::stringify_sized::<256, _>(&my_struct2) {
            // Use panic! to print the parsed struct and the JSON string since
            // we are in no_std land
            panic!("Parsed: {my_struct:?}\nJSON: {}", core::str::from_utf8(&buff[..len]).unwrap());
        }
    }
}

impl<const N: usize> TryFrom<&str> for StringArray<N> {
    type Error = &'static str;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let mut buf = [0; N];
        if s.len() >= N {
            return Err("String too large")
        }
        buf[..s.len()].copy_from_slice(s.as_bytes());
        Ok(StringArray { buf, len: s.len() })
    }
}

impl<const N: usize> core::fmt::Debug for StringArray<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(core::str::from_utf8(&self.buf[..self.len]).unwrap())
    }
}

impl<const N: usize> Serialize for StringArray<N> {
    fn serialize<W: Write>(&self, json: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        json.string_bytes(&self.buf[..self.len])
    }
}

impl<'src, const N: usize> Deserialize<'src, '_> for StringArray<N> {
    fn deserialize(json: &mut Parser<'src>, str_buf: &mut [u8]) -> Result<Self, ParseError> {
        let s = json.string(str_buf)?;
        if s.len() > N {
            let offset = json.error_offset() + N - s.len();
            return Err(ParseError { kind: ParseErrorKind::StringBufferOverflow, offset });
        }
        let mut buf = [0; N];
        buf[..s.len()].copy_from_slice(s.as_bytes());
        Ok(StringArray { buf, len: s.len() })
    }
}
