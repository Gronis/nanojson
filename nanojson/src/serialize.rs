use crate::{Write, WriteError};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum ScopeKind {
    Array,
    Object,
}

#[derive(Copy, Clone)]
struct Scope {
    kind: ScopeKind,
    /// At least one element has been written.
    tail: bool,
    /// An object key was just placed; next write is the value.
    key: bool,
}

/// JSON serializer. Generic over the write sink `W` and maximum nesting depth `DEPTH`.
///
/// # Example
/// ```
/// use nanojson::{Serializer, SliceWriter};
/// let mut buf = [0u8; 64];
/// let mut w = SliceWriter::new(&mut buf);
/// let mut ser: Serializer<_, 32> = Serializer::new(&mut w);
/// ser.object_begin().unwrap();
/// ser.member_key("x").unwrap(); ser.integer(1).unwrap();
/// ser.object_end().unwrap();
/// drop(ser);
/// assert_eq!(w.written(), b"{\"x\":1}");
/// ```
pub struct Serializer<W, const DEPTH: usize = 32> {
    writer: W,
    scopes: [Scope; DEPTH],
    depth: usize,
    /// Pretty-print indent width in spaces. `0` = compact output (default).
    ///
    /// Setting this to a very large value causes the serializer to write that many
    /// space bytes per nesting level. Reasonable values are `2` or `4`.
    pub pp: usize,
}

/// Error type for the serializer: either a write error from the sink, or nesting depth exceeded.
#[derive(Debug)]
pub enum SerializeError<E> {
    Write(E),
    DepthExceeded,
    /// `member_key` / `member_key_bytes` was called outside an object scope,
    /// or was called twice without an intervening value.
    InvalidState,
}

impl<E> From<E> for SerializeError<E> {
    fn from(e: E) -> Self {
        SerializeError::Write(e)
    }
}

impl<E: core::fmt::Display> core::fmt::Display for SerializeError<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SerializeError::Write(e)      => write!(f, "write error: {e}"),
            SerializeError::DepthExceeded => f.write_str("nesting depth exceeded"),
            SerializeError::InvalidState  => f.write_str("invalid serializer call order"),
        }
    }
}

#[cfg(feature = "std")]
impl<E: std::error::Error + 'static> std::error::Error for SerializeError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SerializeError::Write(e) => Some(e),
            _ => None,
        }
    }
}


impl<W: Write, const DEPTH: usize> Serializer<W, DEPTH> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            scopes: [Scope { kind: ScopeKind::Array, tail: false, key: false }; DEPTH],
            depth: 0,
            pp: 0,
        }
    }

    pub fn with_pp(writer: W, indent: usize) -> Self {
        let mut s = Self::new(writer);
        s.pp = indent;
        s
    }

    /// Consume the serializer and return the inner writer.
    pub fn into_writer(self) -> W {
        self.writer
    }

    // ---- internal helpers ----

    fn write(&mut self, b: &[u8]) -> Result<(), SerializeError<W::Error>> {
        self.writer.write_bytes(b).map_err(SerializeError::Write)
    }

    fn current_scope(&mut self) -> Option<&mut Scope> {
        if self.depth > 0 {
            Some(&mut self.scopes[self.depth - 1])
        } else {
            None
        }
    }

    fn element_begin(&mut self) -> Result<(), SerializeError<W::Error>> {
        // We need to read scope fields without holding a mutable borrow on self,
        // so copy out what we need.
        let (tail, key, depth) = if let Some(s) = self.current_scope() {
            (s.tail, s.key, self.depth)
        } else {
            return Ok(());
        };

        if tail && !key {
            self.write(b",")?;
        }

        if self.pp > 0 {
            if key {
                self.write(b" ")?;
            } else {
                self.write(b"\n")?;
                for _ in 0..depth * self.pp {
                    self.write(b" ")?;
                }
            }
        }
        Ok(())
    }

    fn element_end(&mut self) {
        if let Some(s) = self.current_scope() {
            s.tail = true;
            s.key = false;
        }
    }

    fn push_scope(&mut self, kind: ScopeKind) -> Result<(), SerializeError<W::Error>> {
        if self.depth >= DEPTH {
            return Err(SerializeError::DepthExceeded);
        }
        self.scopes[self.depth] = Scope { kind, tail: false, key: false };
        self.depth += 1;
        Ok(())
    }

    fn pop_scope(&mut self) {
        if self.depth > 0 {
            self.depth -= 1;
        }
    }

    fn write_closing(&mut self, close: &[u8]) -> Result<(), SerializeError<W::Error>> {
        let (tail, depth) = if let Some(s) = self.current_scope() {
            (s.tail, self.depth)
        } else {
            (false, 0)
        };
        if self.pp > 0 && tail {
            self.write(b"\n")?;
            for _ in 0..(depth.saturating_sub(1)) * self.pp {
                self.write(b" ")?;
            }
        }
        self.write(close)
    }

    fn write_integer_raw(&mut self, x: i64) -> Result<(), SerializeError<W::Error>> {
        if x < 0 {
            self.write(b"-")?;
            // Avoid overflow for i64::MIN: negate as u64
            let u = if x == i64::MIN {
                (i64::MAX as u64) + 1
            } else {
                (-x) as u64
            };
            return self.write_u64_raw(u);
        }
        self.write_u64_raw(x as u64)
    }

    fn write_u64_raw(&mut self, x: u64) -> Result<(), SerializeError<W::Error>> {
        if x == 0 {
            return self.write(b"0");
        }
        let mut buf = [0u8; 20];
        let mut i = 20usize;
        let mut n = x;
        while n > 0 {
            i -= 1;
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
        }
        self.write(&buf[i..])
    }

    fn write_string_escaped(&mut self, bytes: &[u8]) -> Result<(), SerializeError<W::Error>> {
        self.write(b"\"")?;
        let mut i = 0;
        while i < bytes.len() {
            let ch = bytes[i];
            match ch {
                b'"'  => { self.write(b"\\\"")?; i += 1; }
                b'\\' => { self.write(b"\\\\")?; i += 1; }
                0x08  => { self.write(b"\\b")?;  i += 1; }
                0x09  => { self.write(b"\\t")?;  i += 1; }
                0x0A  => { self.write(b"\\n")?;  i += 1; }
                0x0B  => { self.write(b"\\v")?;  i += 1; }
                0x0C  => { self.write(b"\\f")?;  i += 1; }
                0x0D  => { self.write(b"\\r")?;  i += 1; }
                0x20..=0x7E => {
                    // printable ASCII — emit a run at once
                    let start = i;
                    while i < bytes.len() && matches!(bytes[i], 0x20..=0x7E)
                        && bytes[i] != b'"' && bytes[i] != b'\\'
                    {
                        i += 1;
                    }
                    self.write(&bytes[start..i])?;
                }
                _ => {
                    // Determine UTF-8 sequence length
                    let seq_len = utf8_char_len(ch);
                    if seq_len == 1 {
                        // Non-ASCII single byte (invalid UTF-8 lead) → \u00XX
                        const HEX: &[u8] = b"0123456789abcdef";
                        self.write(b"\\u00")?;
                        self.write(&[HEX[(ch >> 4) as usize], HEX[(ch & 0xF) as usize]])?;
                        i += 1;
                    } else {
                        // Valid multi-byte UTF-8 → passthrough
                        let end = (i + seq_len).min(bytes.len());
                        self.write(&bytes[i..end])?;
                        i = end;
                    }
                }
            }
        }
        self.write(b"\"")
    }

    // ---- public API ----

    pub fn null(&mut self) -> Result<(), SerializeError<W::Error>> {
        self.element_begin()?;
        self.write(b"null")?;
        self.element_end();
        Ok(())
    }

    pub fn bool_val(&mut self, v: bool) -> Result<(), SerializeError<W::Error>> {
        self.element_begin()?;
        self.write(if v { b"true" } else { b"false" })?;
        self.element_end();
        Ok(())
    }

    pub fn integer(&mut self, v: i64) -> Result<(), SerializeError<W::Error>> {
        self.element_begin()?;
        self.write_integer_raw(v)?;
        self.element_end();
        Ok(())
    }

    /// Write a pre-formatted number string verbatim (no escaping).
    /// Use this for floats: format the number yourself and pass the bytes here.
    pub fn number_raw(&mut self, raw: &str) -> Result<(), SerializeError<W::Error>> {
        self.element_begin()?;
        self.write(raw.as_bytes())?;
        self.element_end();
        Ok(())
    }

    pub fn string(&mut self, s: &str) -> Result<(), SerializeError<W::Error>> {
        self.string_bytes(s.as_bytes())
    }

    pub fn string_bytes(&mut self, b: &[u8]) -> Result<(), SerializeError<W::Error>> {
        self.element_begin()?;
        self.write_string_escaped(b)?;
        self.element_end();
        Ok(())
    }

    pub fn array_begin(&mut self) -> Result<(), SerializeError<W::Error>> {
        self.element_begin()?;
        self.write(b"[")?;
        self.push_scope(ScopeKind::Array)?;
        Ok(())
    }

    pub fn array_end(&mut self) -> Result<(), SerializeError<W::Error>> {
        self.write_closing(b"]")?;
        self.pop_scope();
        self.element_end();
        Ok(())
    }

    pub fn object_begin(&mut self) -> Result<(), SerializeError<W::Error>> {
        self.element_begin()?;
        self.write(b"{")?;
        self.push_scope(ScopeKind::Object)?;
        Ok(())
    }

    pub fn member_key(&mut self, key: &str) -> Result<(), SerializeError<W::Error>> {
        self.member_key_bytes(key.as_bytes())
    }

    pub fn member_key_bytes(&mut self, key: &[u8]) -> Result<(), SerializeError<W::Error>> {
        self.element_begin()?;
        match self.current_scope() {
            Some(s) if s.kind == ScopeKind::Object && !s.key => {}
            _ => return Err(SerializeError::InvalidState),
        }
        self.write_string_escaped(key)?;
        self.write(b":")?;
        if let Some(s) = self.current_scope() {
            s.tail = true;
            s.key = true;
        }
        Ok(())
    }

    pub fn object_end(&mut self) -> Result<(), SerializeError<W::Error>> {
        self.write_closing(b"}")?;
        self.pop_scope();
        self.element_end();
        Ok(())
    }
}

fn utf8_char_len(first_byte: u8) -> usize {
    if first_byte & 0x80 == 0 { 1 }
    else if first_byte & 0xE0 == 0xC0 { 2 }
    else if first_byte & 0xF0 == 0xE0 { 3 }
    else if first_byte & 0xF8 == 0xF0 { 4 }
    else { 1 } // invalid lead byte, treat as single
}

/// Trait for types that can serialize themselves as JSON.
pub trait Serialize {
    fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>>;
}

impl Serialize for bool {
    fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        ser.bool_val(*self)
    }
}

macro_rules! impl_integer {
    ($($t:ty),*) => {$(
        impl Serialize for $t {
            fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
                ser.integer(*self as i64)
            }
        }
    )*};
}
impl_integer!(i8, i16, i32, i64, u8, u16, u32, u64, isize, usize);

impl Serialize for str {
    fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        ser.string(self)
    }
}

impl Serialize for &str {
    fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        ser.string(self)
    }
}

#[cfg(feature = "std")]
impl Serialize for std::string::String {
    fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        ser.string(self)
    }
}

impl<T: Serialize> Serialize for Option<T> {
    fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        match self {
            None => ser.null(),
            Some(v) => v.serialize(ser),
        }
    }
}

impl<T: Serialize> Serialize for [T] {
    fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        ser.array_begin()?;
        for item in self {
            item.serialize(ser)?;
        }
        ser.array_end()
    }
}

impl<T: Serialize, const N: usize> Serialize for [T; N] {
    fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        self.as_slice().serialize(ser)
    }
}

impl Serialize for () {
    fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        ser.null()
    }
}

// ---- Convenience free functions ----

/// Serialize via closure into a stack-allocated `[u8; N]`.
/// Returns `(buffer, bytes_written)`.
///
/// # Example
/// ```
/// let (buf, len) = nanojson::stringify_manual_sized::<32>(|s| {
///     s.object_begin()?;
///     s.member_key("n")?; s.integer(7)?;
///     s.object_end()
/// }).unwrap();
/// assert_eq!(&buf[..len], b"{\"n\":7}");
/// ```
#[inline]
pub fn stringify_manual_sized<const N: usize>(
    f: impl FnOnce(&mut Serializer<&mut crate::write::SliceWriter<'_>>) -> Result<(), SerializeError<WriteError>>,
) -> Result<([u8; N], usize), SerializeError<WriteError>> {
    let mut buf = [0u8; N];
    let mut w = crate::write::SliceWriter::new(&mut buf);
    let mut ser = Serializer::new(&mut w);
    f(&mut ser)?;
    let len = w.pos();
    Ok((buf, len))
}

/// Serialize a `T: Serialize` value into a stack-allocated `[u8; N]` buffer.
///
/// # Example
/// ```
/// let (buf, len) = nanojson::stringify_sized::<32, _>(&42i64).unwrap();
/// assert_eq!(&buf[..len], b"42");
/// ```
#[inline]
pub fn stringify_sized<const N: usize, T: Serialize>(
    val: &T,
) -> Result<([u8; N], usize), SerializeError<WriteError>> {
    stringify_manual_sized::<N>(|s| val.serialize(s))
}

/// Serialize via closure into a stack-allocated `[u8; N]` buffer with pretty-printing.
///
/// # Example
/// ```
/// let (buf, len) = nanojson::stringify_manual_sized_pretty::<64>(2, |s| {
///     s.object_begin()?;
///     s.member_key("x")?; s.integer(1)?;
///     s.object_end()
/// }).unwrap();
/// assert_eq!(&buf[..len], b"{\n  \"x\": 1\n}");
/// ```
#[inline]
pub fn stringify_manual_sized_pretty<const N: usize>(
    indent: usize,
    f: impl FnOnce(&mut Serializer<&mut crate::write::SliceWriter<'_>>) -> Result<(), SerializeError<WriteError>>,
) -> Result<([u8; N], usize), SerializeError<WriteError>> {
    let mut buf = [0u8; N];
    let mut w = crate::write::SliceWriter::new(&mut buf);
    let mut ser = Serializer::with_pp(&mut w, indent);
    f(&mut ser)?;
    let len = w.pos();
    Ok((buf, len))
}

/// Serialize a `T: Serialize` value into a stack-allocated `[u8; N]` buffer with pretty-printing.
///
/// # Example
/// ```
/// # use nanojson::Serialize;
/// # #[cfg(feature = "derive")] {
/// #[derive(nanojson::Serialize)]
/// struct Point { x: i64, y: i64 }
/// let (buf, len) = nanojson::stringify_sized_pretty::<64, _>(&Point { x: 1, y: 2 }, 2).unwrap();
/// assert_eq!(&buf[..len], b"{\n  \"x\": 1,\n  \"y\": 2\n}");
/// # }
/// ```
#[inline]
pub fn stringify_sized_pretty<const N: usize, T: Serialize>(
    val: &T,
    indent: usize,
) -> Result<([u8; N], usize), SerializeError<WriteError>> {
    stringify_manual_sized_pretty::<N>(indent, |s| val.serialize(s))
}

/// Count the bytes that a closure would produce without writing anything.
/// Returns the byte count; returns 0 if `DepthExceeded` is hit.
///
/// # Example
/// ```
/// let n = nanojson::measure(|s| {
///     s.object_begin()?;
///     s.member_key("x")?; s.integer(1)?;
///     s.object_end()
/// });
/// assert_eq!(n, 7); // {"x":1}
/// ```
#[inline]
pub fn measure(
    f: impl FnOnce(&mut Serializer<&mut crate::write::SizeCounter>) -> Result<(), SerializeError<core::convert::Infallible>>,
) -> usize {
    let mut counter = crate::write::SizeCounter::new();
    let mut ser = Serializer::new(&mut counter);
    let _ = f(&mut ser);
    counter.count
}

/// Serialize a value into a heap-allocated [`String`].
/// Only fails if nesting exceeds the default depth limit (32).
///
/// # Example
/// ```
/// let json = nanojson::stringify(&[1i64, 2, 3]).unwrap();
/// assert_eq!(json, "[1,2,3]");
/// ```
#[cfg(feature = "std")]
#[inline]
pub fn stringify<T: Serialize>(
    val: &T,
) -> Result<std::string::String, SerializeError<core::convert::Infallible>> {
    stringify_manual(|s| val.serialize(s))
}

/// Serialize via closure into a heap-allocated [`String`].
/// The output buffer grows as needed; no size choice required.
/// Only fails if nesting exceeds the default depth limit (32).
///
/// # Example
/// ```
/// let json = nanojson::stringify_manual(|s| {
///     s.object_begin()?;
///     s.member_key("x")?; s.integer(1)?;
///     s.object_end()
/// }).unwrap();
/// assert_eq!(json, r#"{"x":1}"#);
/// ```
#[cfg(feature = "std")]
#[inline]
pub fn stringify_manual(
    f: impl FnOnce(&mut Serializer<std::vec::Vec<u8>>) -> Result<(), SerializeError<core::convert::Infallible>>,
) -> Result<std::string::String, SerializeError<core::convert::Infallible>> {
    let mut ser: Serializer<_> = Serializer::new(std::vec::Vec::new());
    f(&mut ser)?;
    let vec = ser.into_writer();
    // SAFETY: the serializer only writes valid JSON, which is always valid UTF-8.
    Ok(unsafe { std::string::String::from_utf8_unchecked(vec) })
}

/// Serialize a value into a pretty-printed heap-allocated [`String`].
/// Only fails if nesting exceeds the default depth limit (32).
///
/// # Example
/// ```
/// let json = nanojson::stringify_pretty(&[1i64, 2, 3], 2).unwrap();
/// assert_eq!(json, "[\n  1,\n  2,\n  3\n]");
/// ```
#[cfg(feature = "std")]
#[inline]
pub fn stringify_pretty<T: Serialize>(
    val: &T,
    indent: usize,
) -> Result<std::string::String, SerializeError<core::convert::Infallible>> {
    stringify_manual_pretty(indent, |s| val.serialize(s))
}

/// Serialize via closure into a pretty-printed heap-allocated [`String`].
/// Only fails if nesting exceeds the default depth limit (32).
///
/// # Example
/// ```
/// let json = nanojson::stringify_manual_pretty(2, |s| {
///     s.object_begin()?;
///     s.member_key("x")?; s.integer(1)?;
///     s.object_end()
/// }).unwrap();
/// assert_eq!(json, "{\n  \"x\": 1\n}");
/// ```
#[cfg(feature = "std")]
#[inline]
pub fn stringify_manual_pretty(
    indent: usize,
    f: impl FnOnce(&mut Serializer<std::vec::Vec<u8>>) -> Result<(), SerializeError<core::convert::Infallible>>,
) -> Result<std::string::String, SerializeError<core::convert::Infallible>> {
    let mut ser: Serializer<_> = Serializer::with_pp(std::vec::Vec::new(), indent);
    f(&mut ser)?;
    let vec = ser.into_writer();
    // SAFETY: the serializer only writes valid JSON, which is always valid UTF-8.
    Ok(unsafe { std::string::String::from_utf8_unchecked(vec) })
}
