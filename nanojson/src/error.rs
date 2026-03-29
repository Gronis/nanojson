#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WriteError {
    BufferFull,
    DepthExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    /// Byte offset into the source where the error occurred.
    /// Use this to compute line/column in your own diagnostics code.
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

impl core::fmt::Display for WriteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            WriteError::BufferFull    => f.write_str("output buffer is full"),
            WriteError::DepthExceeded => f.write_str("nesting depth exceeded"),
        }
    }
}

impl core::fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ParseErrorKind::UnexpectedToken { expected, got } =>
                write!(f, "expected {expected}, got {got}"),
            ParseErrorKind::UnexpectedEof =>
                f.write_str("unexpected end of input"),
            ParseErrorKind::InvalidEscape(b) =>
                write!(f, "invalid escape: \\{}", *b as char),
            ParseErrorKind::StringBufferOverflow =>
                f.write_str("string exceeds scratch buffer"),
            ParseErrorKind::InvalidUtf8 =>
                f.write_str("invalid UTF-8 in string"),
            ParseErrorKind::UnknownField =>
                f.write_str("unknown field"),
            ParseErrorKind::MissingField =>
                f.write_str("missing required field"),
        }
    }
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "parse error at offset {}: {}", self.offset, self.kind)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for WriteError {}

#[cfg(feature = "std")]
impl std::error::Error for ParseError {}
