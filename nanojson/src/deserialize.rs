use crate::error::{ParseError, ParseErrorKind};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Token {
    Invalid,
    Eof,
    OpenCurly,
    CloseCurly,
    OpenBracket,
    CloseBracket,
    Comma,
    Colon,
    True,
    False,
    Null,
    String,
    Number,
}

impl Token {
    fn name(self) -> &'static str {
        match self {
            Token::Invalid      => "invalid",
            Token::Eof          => "end of input",
            Token::OpenCurly    => "{",
            Token::CloseCurly   => "}",
            Token::OpenBracket  => "[",
            Token::CloseBracket => "]",
            Token::Comma        => ",",
            Token::Colon        => ":",
            Token::True         => "true",
            Token::False        => "false",
            Token::Null         => "null",
            Token::String       => "string",
            Token::Number       => "number",
        }
    }
}

/// Immediate-mode JSON parser. Borrows the source (`'src`) and a user-supplied
/// scratch buffer (`'buf`) for string unescaping.
///
/// # Example
/// ```ignore
/// let mut str_buf = [0u8; 256];
/// let mut parser = Parser::new(json_bytes, &mut str_buf);
/// parser.object_begin()?;
/// while let Some(key) = parser.object_member()? {
///     match key {
///         "name" => { let s = parser.string()?; }
///         _ => return Err(parser.unknown_field()),
///     }
/// }
/// parser.object_end()?;
/// ```
pub struct Parser<'src, 'buf> {
    src: &'src [u8],
    pos: usize,
    token_start: usize,

    str_buf: &'buf mut [u8],
    str_len: usize,

    token: Token,
    /// Source span of the last NUMBER token (points into `src`).
    number_start: usize,
    number_end: usize,
}

impl<'src, 'buf> Parser<'src, 'buf> {
    pub fn new(src: &'src [u8], str_buf: &'buf mut [u8]) -> Self {
        Self {
            src,
            pos: 0,
            token_start: 0,
            str_buf,
            str_len: 0,
            token: Token::Invalid,
            number_start: 0,
            number_end: 0,
        }
    }

    /// Byte offset of the start of the most recently attempted token.
    /// Use this in your own diagnostics to compute line/column.
    pub fn error_offset(&self) -> usize {
        self.token_start
    }

    // ---- tokenizer ----

    fn skip_whitespace(&mut self) {
        while self.pos < self.src.len() {
            match self.src[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn get_token(&mut self) -> Result<(), ParseError> {
        self.skip_whitespace();
        self.token_start = self.pos;

        if self.pos >= self.src.len() {
            self.token = Token::Eof;
            return Ok(());
        }

        let ch = self.src[self.pos];

        // Single-char punctuation
        let punct = match ch {
            b'{' => Some(Token::OpenCurly),
            b'}' => Some(Token::CloseCurly),
            b'[' => Some(Token::OpenBracket),
            b']' => Some(Token::CloseBracket),
            b',' => Some(Token::Comma),
            b':' => Some(Token::Colon),
            _ => None,
        };
        if let Some(t) = punct {
            self.token = t;
            self.pos += 1;
            return Ok(());
        }

        // Keywords: true / false / null
        let keywords: [(&[u8], Token); 3] = [
            (b"true",  Token::True),
            (b"false", Token::False),
            (b"null",  Token::Null),
        ];
        for (keyword, tok) in keywords {
            if self.src[self.pos..].starts_with(keyword) {
                self.token = tok;
                self.pos += keyword.len();
                return Ok(());
            }
        }

        // Number: optional '-', digits, optional '.digits', optional 'e/E±digits'
        if ch == b'-' || ch.is_ascii_digit() {
            let start = self.pos;
            if ch == b'-' { self.pos += 1; }
            while self.pos < self.src.len() && self.src[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
            if self.pos < self.src.len() && self.src[self.pos] == b'.' {
                self.pos += 1;
                while self.pos < self.src.len() && self.src[self.pos].is_ascii_digit() {
                    self.pos += 1;
                }
            }
            if self.pos < self.src.len() && matches!(self.src[self.pos], b'e' | b'E') {
                self.pos += 1;
                if self.pos < self.src.len() && matches!(self.src[self.pos], b'+' | b'-') {
                    self.pos += 1;
                }
                while self.pos < self.src.len() && self.src[self.pos].is_ascii_digit() {
                    self.pos += 1;
                }
            }
            self.number_start = start;
            self.number_end = self.pos;
            self.token = Token::Number;
            return Ok(());
        }

        // String
        if ch == b'"' {
            self.pos += 1;
            self.str_len = 0;
            let mut has_escapes = false;

            loop {
                if self.pos >= self.src.len() {
                    self.token = Token::Invalid;
                    return Err(ParseError::at(
                        self.token_start,
                        ParseErrorKind::UnexpectedEof,
                    ));
                }
                match self.src[self.pos] {
                    b'"' => {
                        self.pos += 1;
                        self.token = Token::String;
                        let _ = has_escapes;
                        return Ok(());
                    }
                    b'\\' => {
                        has_escapes = true;
                        self.pos += 1;
                        if self.pos >= self.src.len() {
                            self.token = Token::Invalid;
                            return Err(ParseError::at(
                                self.pos,
                                ParseErrorKind::UnexpectedEof,
                            ));
                        }
                        let esc = self.src[self.pos];
                        self.pos += 1;
                        let decoded = match esc {
                            b'"'  => b'"',
                            b'\\' => b'\\',
                            b'/'  => b'/',
                            b'b'  => b'\x08',
                            b't'  => b'\t',
                            b'n'  => b'\n',
                            b'v'  => b'\x0B',
                            b'f'  => b'\x0C',
                            b'r'  => b'\r',
                            other => {
                                self.token = Token::Invalid;
                                return Err(ParseError::at(
                                    self.pos - 1,
                                    ParseErrorKind::InvalidEscape(other),
                                ));
                            }
                        };
                        if self.str_len >= self.str_buf.len() {
                            return Err(ParseError::at(
                                self.token_start,
                                ParseErrorKind::StringBufferOverflow,
                            ));
                        }
                        self.str_buf[self.str_len] = decoded;
                        self.str_len += 1;
                    }
                    _ => {
                        let b = self.src[self.pos];
                        self.pos += 1;
                        if self.str_len >= self.str_buf.len() {
                            return Err(ParseError::at(
                                self.token_start,
                                ParseErrorKind::StringBufferOverflow,
                            ));
                        }
                        self.str_buf[self.str_len] = b;
                        self.str_len += 1;
                    }
                }
            }
        }

        self.token = Token::Invalid;
        Err(ParseError::at(
            self.token_start,
            ParseErrorKind::UnexpectedToken { expected: "value", got: "invalid character" },
        ))
    }

    fn expect_token(&mut self, expected: Token) -> Result<(), ParseError> {
        if self.token != expected {
            return Err(ParseError::at(
                self.token_start,
                ParseErrorKind::UnexpectedToken {
                    expected: expected.name(),
                    got: self.token.name(),
                },
            ));
        }
        Ok(())
    }

    fn get_and_expect(&mut self, expected: Token) -> Result<(), ParseError> {
        self.get_token()?;
        self.expect_token(expected)
    }

    /// After a successful String token, return the decoded string as a `&str`.
    fn current_string(&self) -> Result<&str, ParseError> {
        let bytes = &self.str_buf[..self.str_len];
        core::str::from_utf8(bytes).map_err(|_| {
            ParseError::at(self.token_start, ParseErrorKind::InvalidUtf8)
        })
    }

    // ---- public API ----

    /// Parse `{`.
    pub fn object_begin(&mut self) -> Result<(), ParseError> {
        self.get_and_expect(Token::OpenCurly)
    }

    /// Parse the next object member key, or return `None` when `}` is reached.
    ///
    /// On success with `Some(key)`, the key string is valid for `'buf` (the
    /// lifetime of the scratch buffer). Copy it if you need it longer.
    pub fn object_member(&mut self) -> Result<Option<&'buf str>, ParseError> {
        let saved_pos = self.pos;
        self.get_token()?;

        match self.token {
            Token::Comma => {
                // Subsequent member: expect key
                self.get_and_expect(Token::String)?;
                self.get_and_expect(Token::Colon)?;
                let s = self.current_string()?;
                // SAFETY: we need to return a &str whose lifetime is tied to
                // `'buf` since it lives in str_buf. The borrow checker cannot
                // see this through the method boundary, so we use a raw pointer
                // to extend the lifetime.
                let s: &'buf str = unsafe { core::mem::transmute(s) };
                Ok(Some(s))
            }
            Token::CloseCurly => {
                self.pos = saved_pos;
                Ok(None)
            }
            Token::String => {
                // First member
                self.get_and_expect(Token::Colon)?;
                let s = self.current_string()?;
                let s: &'buf str = unsafe { core::mem::transmute(s) };
                Ok(Some(s))
            }
            _ => Err(ParseError::at(
                self.token_start,
                ParseErrorKind::UnexpectedToken {
                    expected: "string or }",
                    got: self.token.name(),
                },
            )),
        }
    }

    /// Parse `}`.
    pub fn object_end(&mut self) -> Result<(), ParseError> {
        self.get_and_expect(Token::CloseCurly)
    }

    /// Returns an `UnknownField` error at the current position.
    /// Call this inside the `_` arm of your `object_member` match.
    pub fn unknown_field(&self) -> ParseError {
        ParseError::at(self.token_start, ParseErrorKind::UnknownField)
    }

    /// Parse `[`.
    pub fn array_begin(&mut self) -> Result<(), ParseError> {
        self.get_and_expect(Token::OpenBracket)
    }

    /// Check whether there is another item in the array.
    /// Returns `true` if so (consuming a `,` separator if present),
    /// `false` when `]` is reached.
    pub fn array_item(&mut self) -> Result<bool, ParseError> {
        let saved_pos = self.pos;
        self.get_token()?;
        match self.token {
            Token::Comma => Ok(true),
            Token::CloseBracket => {
                self.pos = saved_pos;
                Ok(false)
            }
            _ => {
                // First item or unexpected token — rewind and let the item parser handle it.
                self.pos = saved_pos;
                Ok(true)
            }
        }
    }

    /// Parse `]`.
    pub fn array_end(&mut self) -> Result<(), ParseError> {
        self.get_and_expect(Token::CloseBracket)
    }

    /// Parse `null`.
    pub fn null(&mut self) -> Result<(), ParseError> {
        self.get_and_expect(Token::Null)
    }

    /// Parse `true` or `false`, returning the value.
    pub fn bool_val(&mut self) -> Result<bool, ParseError> {
        self.get_token()?;
        match self.token {
            Token::True  => Ok(true),
            Token::False => Ok(false),
            _ => Err(ParseError::at(
                self.token_start,
                ParseErrorKind::UnexpectedToken { expected: "boolean", got: self.token.name() },
            )),
        }
    }

    /// Parse a JSON string. Returns a `&'buf str` backed by the scratch buffer.
    ///
    /// The returned string is valid for `'buf` (the lifetime of the scratch buffer).
    /// It is overwritten on the next call to `string()` or `object_member()`.
    pub fn string(&mut self) -> Result<&'buf str, ParseError> {
        self.get_and_expect(Token::String)?;
        let s = self.current_string()?;
        // SAFETY: `str_buf` is `&'buf mut [u8]`, so the decoded string lives for
        // `'buf`. The borrow checker cannot express this through `&mut self`, so
        // we transmute to attach the correct lifetime.
        let s: &'buf str = unsafe { core::mem::transmute(s) };
        Ok(s)
    }

    /// Parse a JSON number and return the raw source bytes as a `&'src str`.
    /// No numeric conversion is performed. Parse the value yourself with
    /// e.g. `s.parse::<f64>()` (requires std) or a dedicated crate.
    pub fn number_str(&mut self) -> Result<&'src str, ParseError> {
        self.get_and_expect(Token::Number)?;
        let bytes = &self.src[self.number_start..self.number_end];
        core::str::from_utf8(bytes).map_err(|_| {
            ParseError::at(self.token_start, ParseErrorKind::InvalidUtf8)
        })
    }

    // ---- lookahead ----

    fn peek_token(&mut self) -> Token {
        let saved_pos = self.pos;
        let saved_token_start = self.token_start;
        let saved_token = self.token;
        let _ = self.get_token();
        let peeked = self.token;
        self.pos = saved_pos;
        self.token_start = saved_token_start;
        self.token = saved_token;
        peeked
    }

    pub fn is_null_ahead(&mut self) -> bool   { self.peek_token() == Token::Null }
    pub fn is_bool_ahead(&mut self) -> bool   { matches!(self.peek_token(), Token::True | Token::False) }
    pub fn is_number_ahead(&mut self) -> bool { self.peek_token() == Token::Number }
    pub fn is_string_ahead(&mut self) -> bool { self.peek_token() == Token::String }
    pub fn is_array_ahead(&mut self) -> bool  { self.peek_token() == Token::OpenBracket }
    pub fn is_object_ahead(&mut self) -> bool { self.peek_token() == Token::OpenCurly }
}

/// Trait for types that can deserialize themselves from JSON using a [`Parser`].
pub trait Deserialize<'src, 'buf>: Sized {
    fn deserialize(json: &mut Parser<'src, 'buf>) -> Result<Self, ParseError>;
}

impl<'src, 'buf> Deserialize<'src, 'buf> for bool {
    fn deserialize(json: &mut Parser<'src, 'buf>) -> Result<Self, ParseError> {
        json.bool_val()
    }
}

impl<'src, 'buf> Deserialize<'src, 'buf> for &'buf str {
    fn deserialize(json: &mut Parser<'src, 'buf>) -> Result<Self, ParseError> {
        json.string()
    }
}

#[cfg(feature = "std")]
impl<'src, 'buf> Deserialize<'src, 'buf> for std::string::String {
    fn deserialize(json: &mut Parser<'src, 'buf>) -> Result<Self, ParseError> {
        Ok(std::string::String::from(json.string()?))
    }
}

// Note: there is intentionally NO `Deserialize` impl for `&str` (or
// `&'buf str`). The scratch buffer is reused on every `string()` /
// `object_member()` call, so any `&str` reference into it is invalidated the
// moment the next string is parsed. Providing an impl that returns `&'buf str`
// would appear safe to the borrow checker (since `'buf` covers the whole parse
// session) but would silently hand out stale string slices.
//
// For deserialized strings:
//  - In `no_std` + `no_alloc` contexts: read `parser.string()` immediately and
//    copy the bytes into your own per-field array before calling any further
//    parse method.
//  - When `alloc` is available (add `extern crate alloc`): implement
//    `Deserialize` for `alloc::string::String` yourself.
//  - The `#[derive(Deserialize)]` macro supports string fields; it is
//    the user's responsibility to ensure the scratch buffer is large enough
//    and to copy string values before overwriting.

macro_rules! impl_integer {
    ($($t:ty),*) => {$(
        impl<'src, 'buf> Deserialize<'src, 'buf> for $t {
            fn deserialize(parser: &mut Parser<'src, 'buf>) -> Result<Self, ParseError> {
                // Read the number as a raw string and parse it.
                // integer_from_str is hand-rolled to stay no_std.
                let s = parser.number_str()?;
                integer_from_str::<$t>(s, parser.token_start)
            }
        }
    )*};
}
impl_integer!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);

fn integer_from_str<T: IntParse>(s: &str, offset: usize) -> Result<T, ParseError> {
    T::from_str(s).ok_or_else(|| ParseError::at(
        offset,
        ParseErrorKind::UnexpectedToken { expected: "integer", got: "number out of range" },
    ))
}

trait IntParse: Sized {
    fn from_str(s: &str) -> Option<Self>;
}

macro_rules! impl_int_parse {
    (signed: $($t:ty),*) => {$(
        impl IntParse for $t {
            fn from_str(s: &str) -> Option<Self> {
                let bytes = s.as_bytes();
                if bytes.is_empty() { return None; }
                let (neg, digits) = if bytes[0] == b'-' { (true, &bytes[1..]) } else { (false, bytes) };
                if digits.is_empty() { return None; }
                // Accumulate as a negative value so that i8::MIN (-128) parses correctly.
                // 128 doesn't fit as positive i8 before negation, but -128 fits as-is.
                let mut v: $t = 0;
                for &b in digits {
                    if b < b'0' || b > b'9' { return None; }
                    v = v.checked_mul(10)?.checked_sub((b - b'0') as $t)?;
                }
                if neg { Some(v) } else { v.checked_neg() }
            }
        }
    )*};
    (unsigned: $($t:ty),*) => {$(
        impl IntParse for $t {
            fn from_str(s: &str) -> Option<Self> {
                let bytes = s.as_bytes();
                if bytes.is_empty() || bytes[0] == b'-' { return None; }
                let mut v: $t = 0;
                for &b in bytes {
                    if b < b'0' || b > b'9' { return None; }
                    v = v.checked_mul(10)?.checked_add((b - b'0') as $t)?;
                }
                Some(v)
            }
        }
    )*};
}
impl_int_parse!(signed:   i8, i16, i32, i64, i128, isize);
impl_int_parse!(unsigned: u8, u16, u32, u64, u128, usize);

impl<'src, 'buf, T: Deserialize<'src, 'buf>> Deserialize<'src, 'buf> for Option<T> {
    fn deserialize(parser: &mut Parser<'src, 'buf>) -> Result<Self, ParseError> {
        if parser.is_null_ahead() {
            parser.null()?;
            Ok(None)
        } else {
            T::deserialize(parser).map(Some)
        }
    }
}

// ---- Convenience free functions ----

pub fn parse_manual_sized<'s, const STR_BUF: usize, T>(
    src: &[u8],
    f: impl for<'a, 'b> FnOnce(&mut Parser<'a, 'b>) -> Result<T, ParseError>,
) -> Result<T, ParseError> {
    let mut scratch = [0u8; STR_BUF];
    let mut parser = Parser::new(src, scratch.as_mut_slice());
    f(&mut parser)
}

/// Deserialize a `T: Deserialize` value with a stack-allocated scratch buffer of `STR_BUF` bytes.
#[inline]
pub fn parse_sized<'s, const STR_BUF: usize, T>(
    src: &'s [u8],
) -> Result<T, ParseError>
where
    T: for<'b> Deserialize<'s, 'b>,
{
    let mut buf = [0u8; STR_BUF];
    T::deserialize(&mut Parser::new(src, &mut buf))
}

/// Deserialize a fully-owned type from raw bytes.
/// The scratch buffer is auto-allocated at `src.len()` bytes (safe upper bound
/// for string decoding: a decoded string is never longer than its escaped form).
#[cfg(feature = "std")]
#[inline]
pub fn parse_bytes<T>(src: &[u8]) -> Result<T, ParseError>
where
    T: for<'s, 'b> Deserialize<'s, 'b>,
{
    let mut scratch = std::vec![0u8; src.len().max(1)];
    T::deserialize(&mut Parser::new(src, scratch.as_mut_slice()))
}

/// Deserialize a fully-owned type from a `&str`.
/// The scratch buffer is auto-allocated; no size choice required.
#[cfg(feature = "std")]
#[inline]
pub fn parse<T>(src: &str) -> Result<T, ParseError>
where
    T: for<'s, 'b> Deserialize<'s, 'b>,
{
    parse_bytes(src.as_bytes())
}

/// Drive the parser manually with an auto-sized heap-allocated scratch buffer.
/// The scratch buffer is sized to `src.len()` (safe upper bound for string decoding).
/// `T` must be a fully owned type (no borrows from the parser).
#[cfg(feature = "std")]
#[inline]
pub fn parse_manual<T>(
    src: &[u8],
    f: impl for<'a, 'b> FnOnce(&mut Parser<'a, 'b>) -> Result<T, ParseError>,
) -> Result<T, ParseError> {
    let mut scratch = std::vec![0u8; src.len().max(1)];
    let mut parser = Parser::new(src, scratch.as_mut_slice());
    f(&mut parser)
}
