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
    MissingField { field: &'static str },
}

impl ParseError {
    pub(crate) fn at(offset: usize, kind: ParseErrorKind) -> Self {
        Self { kind, offset }
    }

    /// Returns a display wrapper that renders the error alongside the relevant
    /// portion of `src`, with a `^` pointer at the error position.
    ///
    /// ```
    /// let src = r#"{"x": 1.5}"#;
    /// if let Err(e) = nanojson::parse::<u32>(src) {
    ///     eprintln!("{}", e.display_with_source(src));
    /// }
    /// ```
    pub fn display_with_source<'a>(&'a self, src: &'a str) -> ParseErrorDisplay<'a> {
        ParseErrorDisplay { error: self, src }
    }

    /// Prints a human-readable diagnostic with source context to stderr.
    ///
    /// ```
    /// let src = r#"{"x": 1.5}"#;
    /// if let Err(e) = nanojson::parse::<u32>(src) {
    ///     e.print(src);
    /// }
    /// ```
    #[cfg(feature = "std")]
    pub fn print(&self, src: &str) {
        use std::eprintln;
        eprintln!("{}", self.display_with_source(src));
    }
}

/// A display wrapper that shows a [`ParseError`] with source context.
///
/// Created by [`ParseError::display_with_source`].
pub struct ParseErrorDisplay<'a> {
    error: &'a ParseError,
    src: &'a str,
}

impl core::fmt::Display for ParseErrorDisplay<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let src = self.src;
        let offset = self.error.offset.min(src.len());
        let bytes = src.as_bytes();

        // Find the line containing the error.
        let line_start = bytes[..offset]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |p| p + 1);
        let line_end = bytes[offset..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(src.len(), |p| offset + p);

        let line = &src[line_start..line_end];

        // Column (in chars) of the error within this line.
        let col = src[line_start..offset].chars().count();
        let total_chars = line.chars().count();

        // Window of at most MAX_WIDTH chars centered on the error.
        const MAX_WIDTH: usize = 80;
        let (win_start, win_end) = if total_chars <= MAX_WIDTH {
            (0, total_chars)
        } else {
            let s = col.saturating_sub(MAX_WIDTH / 2);
            let e = (s + MAX_WIDTH).min(total_chars);
            (s, e)
        };

        let left_dots  = win_start > 0;
        let right_dots = win_end < total_chars;

        // Write the (possibly-trimmed) source line.
        if left_dots { f.write_str("...")?; }
        for (i, c) in line.chars().enumerate() {
            if i >= win_end { break; }
            if i >= win_start { write!(f, "{c}")?; }
        }
        if right_dots { f.write_str("...")?; }
        writeln!(f)?;

        // Pointer: chars before error in the window, plus "..." prefix width.
        let pointer = (col - win_start) + if left_dots { 3 } else { 0 };
        for _ in 0..pointer { f.write_str(" ")?; }
        writeln!(f, "^")?;

        // Error description aligned under the pointer.
        for _ in 0..pointer { f.write_str(" ")?; }
        write!(f, "{}", self.error.kind)
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
            ParseErrorKind::MissingField { field } =>
                write!(f, "missing required field `{field}`"),
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
