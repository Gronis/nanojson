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

/// Immediate-mode JSON parser. Borrows the source (`'src`).
///
/// A scratch buffer must be supplied to `string()` and `object_member()` calls
/// for string decoding. This keeps the lifetime relationship explicit: the
/// returned `&str` lives as long as the buffer you pass in.
///
/// # Example
/// ```
/// use nanojson::Parser;
/// let src = b"[1, 2, 3]";
/// let mut p = Parser::new(src);
/// p.array_begin().unwrap();
/// let mut sum = 0i64;
/// while p.array_item().unwrap() {
///     sum += p.number_str().unwrap().parse::<i64>().unwrap();
/// }
/// p.array_end().unwrap();
/// assert_eq!(sum, 6);
/// ```
pub struct Parser<'src> {
    src: &'src [u8],
    pos: usize,
    token_start: usize,
    /// Start of the most recently parsed object member key (the opening `"`).
    /// Used by [`Parser::unknown_field`] to point at the key, not the colon.
    key_start: usize,

    str_len: usize,

    token: Token,
    /// Source span of the last NUMBER token (points into `src`).
    number_start: usize,
    number_end: usize,
}

impl<'src> Parser<'src> {
    pub fn new(src: &'src [u8]) -> Self {
        Self {
            src,
            pos: 0,
            token_start: 0,
            key_start: 0,
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

    /// Tokenize the next token. Writes decoded string bytes into `str_buf` when
    /// the token is a string; for all other tokens `str_buf` is not used.
    /// Pass `&mut []` when you do not expect a string token.
    fn get_token(&mut self, str_buf: &mut [u8]) -> Result<(), ParseError> {
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
                        return Ok(());
                    }
                    b'\\' => {
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
                        if esc == b'u' {
                            // \uXXXX — parse 4 hex digits then encode as UTF-8.
                            if self.pos + 4 > self.src.len() {
                                return Err(ParseError::at(self.pos, ParseErrorKind::UnexpectedEof));
                            }
                            let h = parse_hex4(&self.src[self.pos..])
                                .ok_or_else(|| ParseError::at(self.pos, ParseErrorKind::InvalidEscape(b'u')))?;
                            self.pos += 4;

                            let cp: u32 = if (0xD800..=0xDBFF).contains(&h) {
                                // High surrogate — must be followed by \uDC00..=\uDFFF.
                                if self.pos + 6 > self.src.len()
                                    || self.src[self.pos]     != b'\\'
                                    || self.src[self.pos + 1] != b'u'
                                {
                                    return Err(ParseError::at(self.pos, ParseErrorKind::InvalidEscape(b'u')));
                                }
                                self.pos += 2;
                                let low = parse_hex4(&self.src[self.pos..])
                                    .ok_or_else(|| ParseError::at(self.pos, ParseErrorKind::InvalidEscape(b'u')))?;
                                if !(0xDC00..=0xDFFF).contains(&low) {
                                    return Err(ParseError::at(self.pos, ParseErrorKind::InvalidEscape(b'u')));
                                }
                                self.pos += 4;
                                0x10000 + ((h as u32 - 0xD800) << 10) + (low as u32 - 0xDC00)
                            } else if (0xDC00..=0xDFFF).contains(&h) {
                                // Lone low surrogate.
                                return Err(ParseError::at(self.pos - 4, ParseErrorKind::InvalidEscape(b'u')));
                            } else {
                                h as u32
                            };

                            let (bytes, len) = encode_utf8_cp(cp);
                            for &byte in &bytes[..len] {
                                if self.str_len >= str_buf.len() {
                                    return Err(ParseError::at(
                                        self.token_start,
                                        ParseErrorKind::StringBufferOverflow,
                                    ));
                                }
                                str_buf[self.str_len] = byte;
                                self.str_len += 1;
                            }
                        } else {
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
                            if self.str_len >= str_buf.len() {
                                return Err(ParseError::at(
                                    self.token_start,
                                    ParseErrorKind::StringBufferOverflow,
                                ));
                            }
                            str_buf[self.str_len] = decoded;
                            self.str_len += 1;
                        }
                    }
                    _ => {
                        let b = self.src[self.pos];
                        self.pos += 1;
                        if self.str_len < str_buf.len() {
                            str_buf[self.str_len] = b;
                            self.str_len += 1;
                        } else if str_buf.len() > 0 {
                            return Err(ParseError::at(
                                self.token_start,
                                ParseErrorKind::StringBufferOverflow,
                            ));
                        }
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

    /// Tokenize and expect a specific non-string token. Passes an empty buffer
    /// because no string decoding is needed for structural tokens.
    fn get_and_expect(&mut self, expected: Token) -> Result<(), ParseError> {
        self.get_token(&mut [])?;
        self.expect_token(expected)
    }

    /// After a successful String token, return the decoded bytes as a `&'b str`.
    /// The lifetime `'b` is tied to `str_buf`, not to `&self`, so this is safe
    /// with no raw pointer tricks.
    fn current_string<'b>(&self, str_buf: &'b [u8]) -> Result<&'b str, ParseError> {
        let bytes = &str_buf[..self.str_len];
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
    /// The returned string is decoded into `str_buf` and is valid for `'b`
    /// (the lifetime of the buffer). Copy it or process it before the next call
    /// that takes a `str_buf`.
    pub fn object_member<'b>(&mut self, str_buf: &'b mut [u8]) -> Result<Option<&'b str>, ParseError> {
        let saved_pos = self.pos;
        self.get_token(str_buf)?;

        match self.token {
            Token::Comma => {
                // Subsequent member: expect key string
                self.get_token(str_buf)?;
                self.expect_token(Token::String)?;
                self.key_start = self.token_start;
                self.get_and_expect(Token::Colon)?;
                Ok(Some(self.current_string(str_buf)?))
            }
            Token::CloseCurly => {
                self.pos = saved_pos;
                Ok(None)
            }
            Token::String => {
                // First member — key was decoded into str_buf by get_token
                self.key_start = self.token_start;
                self.get_and_expect(Token::Colon)?;
                Ok(Some(self.current_string(str_buf)?))
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
        ParseError::at(self.key_start, ParseErrorKind::UnknownField { type_name: "", expected_fields: &[] })
    }

    /// Returns an `UnknownField` error enriched with the type name and its valid field names.
    /// Used by derive-generated code to produce more helpful diagnostics.
    pub fn unknown_field_in(&self, type_name: &'static str, expected_fields: &'static [&'static str]) -> ParseError {
        ParseError::at(self.key_start, ParseErrorKind::UnknownField { type_name, expected_fields })
    }

    /// Parse `[`.
    pub fn array_begin(&mut self) -> Result<(), ParseError> {
        self.get_and_expect(Token::OpenBracket)
    }

    /// Check whether there is another item in the array.
    /// Returns `true` if so (consuming a `,` separator if present),
    /// `false` when `]` is reached.
    ///
    /// Uses fast first-character inspection so no scratch buffer is needed.
    pub fn array_item(&mut self) -> Result<bool, ParseError> {
        match self.peek_token() {
            Token::Comma => {
                // Consume the comma
                self.skip_whitespace();
                self.token_start = self.pos;
                self.pos += 1;
                self.token = Token::Comma;
                Ok(true)
            }
            Token::CloseBracket => Ok(false),
            // First item, or EOF/invalid — let the item deserializer produce the error.
            _ => Ok(true),
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
        self.get_token(&mut [])?;
        match self.token {
            Token::True  => Ok(true),
            Token::False => Ok(false),
            _ => Err(ParseError::at(
                self.token_start,
                ParseErrorKind::UnexpectedToken { expected: "boolean", got: self.token.name() },
            )),
        }
    }

    /// Parse a JSON string, decoding escape sequences into `str_buf`.
    ///
    /// The returned `&'b str` is valid for the lifetime of `str_buf`. It is
    /// overwritten on the next call to `string()` or `object_member()` that
    /// uses the same buffer.
    pub fn string<'b>(&mut self, str_buf: &'b mut [u8]) -> Result<&'b str, ParseError> {
        self.get_token(str_buf)?;
        self.expect_token(Token::String)?;
        self.current_string(str_buf)
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

    /// Peek at the type of the next token without advancing the parser.
    /// Inspects only the first non-whitespace byte, so no scratch buffer is needed.
    fn peek_token(&self) -> Token {
        let mut i = self.pos;
        while i < self.src.len() && matches!(self.src[i], b' ' | b'\t' | b'\n' | b'\r') {
            i += 1;
        }
        if i >= self.src.len() { return Token::Eof; }
        match self.src[i] {
            b'{' => Token::OpenCurly,
            b'}' => Token::CloseCurly,
            b'[' => Token::OpenBracket,
            b']' => Token::CloseBracket,
            b',' => Token::Comma,
            b':' => Token::Colon,
            b'"' => Token::String,
            b't' => Token::True,
            b'f' => Token::False,
            b'n' => Token::Null,
            b'-' | b'0'..=b'9' => Token::Number,
            _ => Token::Invalid,
        }
    }

    pub fn is_null_ahead(&self) -> bool   { self.peek_token() == Token::Null }
    pub fn is_bool_ahead(&self) -> bool   { matches!(self.peek_token(), Token::True | Token::False) }
    pub fn is_number_ahead(&self) -> bool { self.peek_token() == Token::Number }
    pub fn is_string_ahead(&self) -> bool { self.peek_token() == Token::String }
    pub fn is_array_ahead(&self) -> bool  { self.peek_token() == Token::OpenBracket }
    pub fn is_object_ahead(&self) -> bool { self.peek_token() == Token::OpenCurly }
}

/// Trait for types that can deserialize themselves from JSON using a [`Parser`].
///
/// `'src` is the lifetime of the JSON source bytes. `'buf` is the lifetime of
/// the scratch buffer used for string decoding. Owned types implement
/// `for<'s, 'b> Deserialize<'s, 'b>`.
pub trait Deserialize<'src, 'buf>: Sized {
    fn deserialize(parser: &mut Parser<'src>, str_buf: &'buf mut [u8]) -> Result<Self, ParseError>;
}

impl<'src, 'buf> Deserialize<'src, 'buf> for bool {
    fn deserialize(parser: &mut Parser<'src>, _str_buf: &'buf mut [u8]) -> Result<Self, ParseError> {
        parser.bool_val()
    }
}

impl<'src, 'buf> Deserialize<'src, 'buf> for &'buf str {
    fn deserialize(parser: &mut Parser<'src>, str_buf: &'buf mut [u8]) -> Result<Self, ParseError> {
        parser.string(str_buf)
    }
}

#[cfg(feature = "alloc")]
impl<'src, 'buf> Deserialize<'src, 'buf> for alloc::string::String {
    fn deserialize(parser: &mut Parser<'src>, str_buf: &'buf mut [u8]) -> Result<Self, ParseError> {
        Ok(alloc::string::String::from(parser.string(str_buf)?))
    }
}

macro_rules! impl_float {
    ($($t:ty),*) => {$(
        impl<'src, 'buf> Deserialize<'src, 'buf> for $t {
            fn deserialize(parser: &mut Parser<'src>, _str_buf: &'buf mut [u8]) -> Result<Self, ParseError> {
                let s = parser.number_str()?;
                let offset = parser.error_offset();
                s.parse::<$t>().map_err(|_| ParseError::at(
                    offset,
                    ParseErrorKind::UnexpectedToken { expected: "number", got: "invalid float" },
                ))
            }
        }
    )*};
}
impl_float!(f32, f64);

macro_rules! impl_integer {
    ($($t:ty),*) => {$(
        impl<'src, 'buf> Deserialize<'src, 'buf> for $t {
            fn deserialize(parser: &mut Parser<'src>, _str_buf: &'buf mut [u8]) -> Result<Self, ParseError> {
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
    fn deserialize(parser: &mut Parser<'src>, str_buf: &'buf mut [u8]) -> Result<Self, ParseError> {
        if parser.is_null_ahead() {
            parser.null()?;
            Ok(None)
        } else {
            T::deserialize(parser, str_buf).map(Some)
        }
    }
}

impl<'src, 'buf, T, const N: usize> Deserialize<'src, 'buf> for [T; N]
where
    T: for<'x> Deserialize<'src, 'x>,
{
    fn deserialize(parser: &mut Parser<'src>, str_buf: &'buf mut [u8]) -> Result<Self, ParseError> {
        parser.array_begin()?;

        let mut arr: [Option<T>; N] = [(); N].map(|_| None);

        for i in 0..N {
            if !parser.array_item()? {
                return Err(ParseError::at(
                    parser.error_offset(),
                    ParseErrorKind::UnexpectedToken { expected: "array item", got: "]" },
                ));
            }
            // `&mut *str_buf` creates a fresh shorter-lived reborrow so NLL can
            // release it after the call (T: for<'x> ensures T doesn't capture it).
            arr[i] = Some(T::deserialize(parser, &mut *str_buf)?);
        }

        // Reject arrays with more items than N.
        if parser.array_item()? {
            return Err(ParseError::at(
                parser.error_offset(),
                ParseErrorKind::UnexpectedToken { expected: "]", got: "array item" },
            ));
        }
        parser.array_end()?;

        // Every element was set to `Some(...)` by the loop above — unwrap is
        // dead code (the loop invariant guarantees all slots are filled).
        // `array::try_from_fn` would be the ideal solution but is nightly-only.
        Ok(arr.map(|x| x.unwrap()))
    }
}

#[cfg(feature = "arrayvec")]
impl<'src, 'buf, T, const N: usize> Deserialize<'src, 'buf> for arrayvec::ArrayVec<T, N>
where
    T: for<'x> Deserialize<'src, 'x>,
{
    fn deserialize(parser: &mut Parser<'src>, str_buf: &'buf mut [u8]) -> Result<Self, ParseError> {
        let mut vec = arrayvec::ArrayVec::new();
        parser.array_begin()?;
        while parser.array_item()? {
            let v = T::deserialize(parser, &mut *str_buf)?;
            vec.try_push(v).map_err(|_| ParseError::at(
                parser.error_offset(),
                ParseErrorKind::StringBufferOverflow,
            ))?;
        }
        parser.array_end()?;
        Ok(vec)
    }
}

#[cfg(feature = "arrayvec")]
impl<'src, 'buf, const N: usize> Deserialize<'src, 'buf> for arrayvec::ArrayString<N> {
    fn deserialize(parser: &mut Parser<'src>, str_buf: &'buf mut [u8]) -> Result<Self, ParseError> {
        let s = parser.string(str_buf)?;
        arrayvec::ArrayString::try_from(s).map_err(|_| ParseError::at(
            parser.error_offset(),
            ParseErrorKind::StringBufferOverflow,
        ))
    }
}

#[cfg(feature = "alloc")]
impl<'src, 'buf, T> Deserialize<'src, 'buf> for alloc::vec::Vec<T>
where
    T: for<'x> Deserialize<'src, 'x>,
{
    fn deserialize(parser: &mut Parser<'src>, str_buf: &'buf mut [u8]) -> Result<Self, ParseError> {
        let mut vec = alloc::vec::Vec::new();
        parser.array_begin()?;
        while parser.array_item()? {
            vec.push(T::deserialize(parser, &mut *str_buf)?);
        }
        parser.array_end()?;
        Ok(vec)
    }
}

#[cfg(feature = "alloc")]
impl<'src, 'buf, T: Deserialize<'src, 'buf>> Deserialize<'src, 'buf> for alloc::boxed::Box<T> {
    fn deserialize(parser: &mut Parser<'src>, str_buf: &'buf mut [u8]) -> Result<Self, ParseError> {
        T::deserialize(parser, str_buf).map(alloc::boxed::Box::new)
    }
}

macro_rules! impl_deserialize_map {
    ($map_ty:ty, $new:expr) => {
        impl<'src, 'buf, V> Deserialize<'src, 'buf> for $map_ty
        where
            V: for<'x> Deserialize<'src, 'x>,
        {
            fn deserialize(parser: &mut Parser<'src>, str_buf: &'buf mut [u8]) -> Result<Self, ParseError> {
                let mut map = $new;
                parser.object_begin()?;
                // Explicit loop so NLL can see the borrow from object_member ends
                // after String::from(k) before the next reborrow.
                loop {
                    let maybe_key = parser.object_member(&mut *str_buf)?;
                    let key = match maybe_key {
                        None => break,
                        Some(k) => alloc::string::String::from(k),
                    };
                    let value = V::deserialize(parser, &mut *str_buf)?;
                    map.insert(key, value);
                }
                parser.object_end()?;
                Ok(map)
            }
        }
    };
}

#[cfg(feature = "alloc")]
impl_deserialize_map!(
    alloc::collections::BTreeMap<alloc::string::String, V>,
    alloc::collections::BTreeMap::new()
);

#[cfg(feature = "std")]
impl_deserialize_map!(
    std::collections::HashMap<std::string::String, V>,
    std::collections::HashMap::new()
);

// ---- Convenience free functions ----

/// Parse using a hand-written closure with a stack-allocated scratch buffer of `STR_BUF` bytes.
///
/// The closure receives both a `&mut Parser` and a `&mut [u8]` scratch buffer.
/// Pass the buffer to `string()` and `object_member()` calls.
///
/// # Example
/// ```
/// let (x, y) = nanojson::parse_manual_sized(&mut [0u8; 16], b"{\"x\":3,\"y\":4}", |p, buf| {
///     p.object_begin()?;
///     let mut x = 0i64; let mut y = 0i64;
///     while let Some(k) = p.object_member(buf)? {
///         match k {
///             "x" => x = p.number_str()?.parse().unwrap(),
///             "y" => y = p.number_str()?.parse().unwrap(),
///             _ => return Err(p.unknown_field()),
///         }
///     }
///     p.object_end()?;
///     Ok((x, y))
/// }).unwrap();
/// assert_eq!((x, y), (3, 4));
/// ```
pub fn parse_manual_sized<T>(
    buf: &mut [u8],
    src: impl AsRef<[u8]>,
    f: impl for<'a, 'b> FnOnce(&mut Parser<'a>, &'b mut [u8]) -> Result<T, ParseError>,
) -> Result<T, ParseError> {
    let mut parser = Parser::new(src.as_ref());
    f(&mut parser, buf)
}

/// Deserialize a `T: Deserialize` value with a stack-allocated scratch buffer of `STR_BUF` bytes.
///
/// # Example
/// ```
/// let n: i64 = nanojson::parse_sized(&mut [0; 0], b"42").unwrap();
/// assert_eq!(n, 42);
/// ```
#[inline]
pub fn parse_sized<T: for<'s, 'b> Deserialize<'s, 'b>>(
    buf: &mut [u8],
    src: impl AsRef<[u8]>
) -> Result<T, ParseError> {
    T::deserialize(&mut Parser::new(src.as_ref()), buf)
}

/// Deserialize a fully-owned type from raw bytes or `&str`.
/// The scratch buffer is auto-allocated at `src.len()` bytes (safe upper bound
/// for string decoding: a decoded string is never longer than its escaped form).
///
/// # Example
/// ```
/// let n: i64 = nanojson::parse(b"42").unwrap();
/// assert_eq!(n, 42);
///
/// let n: i64 = nanojson::parse("42").unwrap();
/// assert_eq!(n, 42);
/// ```
#[cfg(feature = "std")]
#[inline]
pub fn parse<T: for<'s, 'b> Deserialize<'s, 'b>,>(
    src: impl AsRef<[u8]>,
) -> Result<T, ParseError> {
    let src = src.as_ref();
    let mut scratch = std::vec![0u8; src.len().max(1)];
    T::deserialize(&mut Parser::new(src), scratch.as_mut_slice())
}

/// Drive the parser manually with an auto-sized heap-allocated scratch buffer.
/// The scratch buffer is sized to `src.len()` (safe upper bound for string decoding).
/// `T` must be a fully owned type (no borrows from the parser).
///
/// The closure receives both a `&mut Parser` and a `&mut [u8]` scratch buffer.
/// Pass the buffer to `string()` and `object_member()` calls.
///
/// # Example
/// ```
/// let (x, y) = nanojson::parse_manual(b"{\"x\":3,\"y\":4}", |p, buf| {
///     p.object_begin()?;
///     let mut x = 0i64; let mut y = 0i64;
///     while let Some(k) = p.object_member(buf)? {
///         match k {
///             "x" => x = p.number_str()?.parse().unwrap(),
///             "y" => y = p.number_str()?.parse().unwrap(),
///             _ => return Err(p.unknown_field()),
///         }
///     }
///     p.object_end()?;
///     Ok((x, y))
/// }).unwrap();
/// assert_eq!((x, y), (3, 4));
/// ```
#[cfg(feature = "std")]
#[inline]
pub fn parse_manual<T>(
    src: impl AsRef<[u8]>,
    f: impl for<'a, 'b> FnOnce(&mut Parser<'a>, &'b mut [u8]) -> Result<T, ParseError>,
) -> Result<T, ParseError> {
    let src = src.as_ref();
    let mut scratch = std::vec![0u8; src.len().max(1)];
    let mut parser = Parser::new(src);
    f(&mut parser, &mut scratch)
}

// ---- Unicode helpers (used by \uXXXX parsing in get_token) ----

/// Parse exactly 4 hex digits from the start of `bytes`, returning the u16 value.
/// Returns `None` if fewer than 4 bytes are present or any byte is not a hex digit.
fn parse_hex4(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 4 { return None; }
    let mut n: u16 = 0;
    for &b in &bytes[..4] {
        let d: u16 = match b {
            b'0'..=b'9' => (b - b'0') as u16,
            b'a'..=b'f' => (b - b'a' + 10) as u16,
            b'A'..=b'F' => (b - b'A' + 10) as u16,
            _ => return None,
        };
        n = n * 16 + d;
    }
    Some(n)
}

/// Encode a Unicode codepoint (must be a valid scalar value) as UTF-8.
/// Returns the bytes and the number of bytes written (1–4).
fn encode_utf8_cp(cp: u32) -> ([u8; 4], usize) {
    match cp {
        0x00..=0x7F => ([cp as u8, 0, 0, 0], 1),
        0x80..=0x7FF => ([
            0xC0 | (cp >> 6) as u8,
            0x80 | (cp & 0x3F) as u8,
            0, 0,
        ], 2),
        0x800..=0xFFFF => ([
            0xE0 | (cp >> 12) as u8,
            0x80 | ((cp >> 6) & 0x3F) as u8,
            0x80 | (cp & 0x3F) as u8,
            0,
        ], 3),
        _ => ([  // 0x10000..=0x10FFFF
            0xF0 | (cp >> 18) as u8,
            0x80 | ((cp >> 12) & 0x3F) as u8,
            0x80 | ((cp >> 6) & 0x3F) as u8,
            0x80 | (cp & 0x3F) as u8,
        ], 4),
    }
}
