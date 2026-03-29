#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WriteError {
    BufferFull,
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
    /// ```rust,ignore
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
    /// ```rust,ignore
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
        // Visual column after expanding tabs to 4-space tab stops.
        fn visual_width(s: &str) -> usize {
            s.chars().fold(0, |acc, c| if c == '\t' { (acc / 4 + 1) * 4 } else { acc + 1 })
        }
        fn count_digits(mut n: usize) -> usize {
            if n == 0 { return 1; }
            let mut d = 0;
            while n > 0 { d += 1; n /= 10; }
            d
        }

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

        // Strip Windows \r from line end.
        let raw_line = &src[line_start..line_end];
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);

        // 1-based line number.
        let line_number = bytes[..line_start].iter().filter(|&&b| b == b'\n').count() + 1;

        // Visual column of the error (tab-aware) and total visual width of the line.
        let visual_col   = visual_width(&src[line_start..offset]);
        let total_visual = visual_width(line);

        // Window of at most MAX_WIDTH visual columns centered on the error.
        const MAX_WIDTH: usize = 80;
        let (win_start, win_end) = if total_visual <= MAX_WIDTH {
            (0, total_visual)
        } else {
            let s = visual_col.saturating_sub(MAX_WIDTH / 2);
            let e = (s + MAX_WIDTH).min(total_visual);
            (s, e)
        };

        let left_dots  = win_start > 0;
        let right_dots = win_end < total_visual;

        // Source line with gutter: "10 | {snippet}"
        let digits = count_digits(line_number);
        write!(f, "{} | ", line_number)?;
        if left_dots { f.write_str("...")?; }
        let mut vis = 0usize;
        for c in line.chars() {
            if vis >= win_end { break; }
            let w = if c == '\t' { (vis / 4 + 1) * 4 - vis } else { 1 };
            if vis >= win_start {
                if c == '\t' {
                    for _ in 0..w { f.write_str(" ")?; }
                } else {
                    write!(f, "{c}")?;
                }
            }
            vis += w;
        }
        if right_dots { f.write_str("...")?; }
        writeln!(f)?;

        // Column of the pointer within the displayed snippet.
        let col_in_window = (visual_col - win_start) + if left_dots { 3 } else { 0 };

        // Pointer line: "   |         ^"
        for _ in 0..digits { f.write_str(" ")?; }
        f.write_str(" | ")?;
        for _ in 0..col_in_window { f.write_str(" ")?; }
        writeln!(f, "^")?;

        // Error message: "   |         expected ..."
        for _ in 0..digits { f.write_str(" ")?; }
        f.write_str(" | ")?;
        for _ in 0..col_in_window { f.write_str(" ")?; }
        write!(f, "{}", self.error.kind)
    }
}


impl core::fmt::Display for WriteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            WriteError::BufferFull => f.write_str("output buffer is full"),
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
