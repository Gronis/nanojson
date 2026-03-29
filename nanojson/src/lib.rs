#![no_std]
#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod error;
pub mod write;
pub mod serialize;
pub mod deserialize;

pub use error::{ParseError, ParseErrorDisplay, ParseErrorKind, WriteError};
pub use write::{Write, SliceWriter, SizeCounter};
pub use serialize::{Serializer, Serialize, SerializeError};
pub use deserialize::{Parser, Deserialize};

pub use serialize::{stringify_sized, stringify_manual_sized, stringify_sized_pretty, stringify_manual_sized_pretty, measure};
pub use deserialize::{parse_sized, parse_manual_sized};

#[cfg(feature = "std")]
pub use serialize::{stringify, stringify_manual, stringify_pretty, stringify_manual_pretty};
#[cfg(feature = "std")]
pub use deserialize::{parse, parse_manual, parse_bytes};

#[cfg(feature = "derive")]
pub use nanojson_derive::{Deserialize, Serialize};
