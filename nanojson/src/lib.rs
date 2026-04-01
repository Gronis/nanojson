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

pub use serialize::{stringify_sized, stringify_sized_as, stringify_sized_pretty, stringify_sized_pretty_as, measure};
pub use deserialize::{parse_sized, parse_sized_as};

#[cfg(feature = "std")]
pub use serialize::{stringify, stringify_as, stringify_pretty, stringify_pretty_as,
                    stringify_smart_pretty, stringify_smart_pretty_as,
                    SmartSerializer};
#[cfg(feature = "std")]
pub use deserialize::{parse, parse_as};

#[cfg(feature = "derive")]
pub use nanojson_derive::{Deserialize, Serialize};
