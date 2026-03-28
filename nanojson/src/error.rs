#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    BufferFull,
    DepthExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    /// Byte offset into the source where the error occurred.
    /// Use this to compute line/column in your own diagnostics code.
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    UnexpectedToken {
        expected: &'static str,
        got: &'static str,
    },
    UnexpectedEof,
    InvalidEscape(u8),
    StringBufferOverflow,
    InvalidUtf8,
    UnknownField,
    MissingField,
}

impl ParseError {
    pub(crate) fn at(offset: usize, kind: ParseErrorKind) -> Self {
        Self { kind, offset }
    }
}
