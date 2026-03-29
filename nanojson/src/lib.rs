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

pub use serialize::{stringify_sized, stringify_manual_sized, measure};
pub use deserialize::{parse_sized, parse_manual_sized};

#[cfg(feature = "std")]
pub use serialize::{stringify, stringify_manual};
#[cfg(feature = "std")]
pub use deserialize::{parse, parse_manual, parse_bytes};

pub use nanojson_derive::{Deserialize, Serialize};
