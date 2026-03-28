#![no_std]
#[cfg(feature = "std")]
extern crate std;

pub mod error;
pub mod write;
pub mod serialize;
pub mod deserialize;

#[cfg(test)]
mod tests;

pub use error::{ParseError, ParseErrorKind, WriteError};
pub use write::{Write, SliceWriter, SizeCounter};
pub use serialize::{Serializer, Serialize, SerializeError};
pub use deserialize::{Parser, Deserialize};

pub use serialize::{serialize, to_json, measure};
pub use deserialize::{parse, from_json};

#[cfg(feature = "std")]
pub use serialize::{to_string, serialize_to_string};
#[cfg(feature = "std")]
pub use deserialize::{from_bytes, from_str, parse_dyn};
