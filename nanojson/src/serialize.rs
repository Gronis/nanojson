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
    /// The serializer generated invalid UTF-8 at some point.
    InvalidUtf8(usize),
    /// A value cannot be represented as JSON (e.g. a NaN or infinite float).
    InvalidValue(&'static str),
}

impl<E> From<E> for SerializeError<E> {
    fn from(e: E) -> Self {
        SerializeError::Write(e)
    }
}

impl<E: core::fmt::Display> core::fmt::Display for SerializeError<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Write(e)          => write!(f, "write error: {e}"),
            Self::DepthExceeded     => f.write_str("nesting depth exceeded"),
            Self::InvalidState      => f.write_str("invalid serializer call order"),
            Self::InvalidValue(m)   => write!(f, "invalid value: {m}"),
            Self::InvalidUtf8(off)  => write!(f, "invalid utf-8 generated at: {off}"),
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

    fn write_u128_raw(&mut self, x: u128) -> Result<(), SerializeError<W::Error>> {
        if x == 0 {
            return self.write(b"0");
        }
        let mut buf = [0u8; 39]; // u128::MAX has 39 decimal digits
        let mut i = 39usize;
        let mut n = x;
        while n > 0 {
            i -= 1;
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
        }
        self.write(&buf[i..])
    }

    fn write_i128_raw(&mut self, x: i128) -> Result<(), SerializeError<W::Error>> {
        if x < 0 {
            self.write(b"-")?;
            let u = if x == i128::MIN {
                (i128::MAX as u128) + 1
            } else {
                (-x) as u128
            };
            return self.write_u128_raw(u);
        }
        self.write_u128_raw(x as u128)
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
                0x0B  => { self.escape_byte(ch)?;  i += 1; }
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
                    let seq_len = utf8_char_len(ch);

                    // Fast path: clearly invalid lead or not enough bytes
                    if seq_len == 1 || i + seq_len > bytes.len() {
                        self.escape_byte(ch)?;
                        i += 1;
                        continue;
                    }

                    // Validate continuation bytes: must be 10xxxxxx
                    let mut valid = true;
                    for j in 1..seq_len {
                        if (bytes[i + j] & 0b1100_0000) != 0b1000_0000 {
                            valid = false;
                            break;
                        }
                    }

                    if valid {
                        // Structurally valid UTF-8 → passthrough
                        self.write(&bytes[i..i + seq_len])?;
                        i += seq_len;
                    } else {
                        // Invalid sequence → escape first byte only
                        self.escape_byte(ch)?;
                        i += 1;
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

    pub fn unsigned(&mut self, v: u64) -> Result<(), SerializeError<W::Error>> {
        self.element_begin()?;
        self.write_u64_raw(v)?;
        self.element_end();
        Ok(())
    }

    pub fn integer128(&mut self, v: i128) -> Result<(), SerializeError<W::Error>> {
        self.element_begin()?;
        self.write_i128_raw(v)?;
        self.element_end();
        Ok(())
    }

    pub fn unsigned128(&mut self, v: u128) -> Result<(), SerializeError<W::Error>> {
        self.element_begin()?;
        self.write_u128_raw(v)?;
        self.element_end();
        Ok(())
    }

    /// Write a pre-formatted number string verbatim (no escaping).
    /// Use this for floats: format the number yourself and pass the bytes here.
    pub fn number_raw(&mut self, raw: &[u8]) -> Result<(), SerializeError<W::Error>> {
        self.element_begin()?;
        self.write(raw)?;
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

    fn escape_byte(&mut self, ch: u8) -> Result<(), SerializeError<W::Error>> {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        self.write(b"\\u00")?;
        self.write(&[
            HEX[(ch >> 4) as usize],
            HEX[(ch & 0x0F) as usize],
        ])?;
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
impl_integer!(i8, i16, i32, i64, u8, u16, u32, isize);

// u64 and usize need unsigned() to avoid silent truncation of values > i64::MAX.
impl Serialize for u64 {
    fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        ser.unsigned(*self)
    }
}
impl Serialize for usize {
    fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        ser.unsigned(*self as u64)
    }
}
impl Serialize for i128 {
    fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        ser.integer128(*self)
    }
}
impl Serialize for u128 {
    fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        ser.unsigned128(*self)
    }
}

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

#[cfg(feature = "alloc")]
impl Serialize for alloc::string::String {
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

fn serialize_float<W: Write>(ser: &mut Serializer<W>, v: f64) -> Result<(), SerializeError<W::Error>> {
    if !v.is_finite() {
        return Err(SerializeError::InvalidValue("float must be finite (not NaN or Infinity)"));
    }
    // Format into a 32-byte stack buffer via core::fmt::Write — no alloc needed.
    let mut buf = [0u8; 32];
    let mut pos = 0usize;
    struct FloatBuf<'a>(&'a mut [u8], &'a mut usize);
    impl core::fmt::Write for FloatBuf<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let b = s.as_bytes();
            let end = *self.1 + b.len();
            if end > self.0.len() { return Err(core::fmt::Error); }
            self.0[*self.1..end].copy_from_slice(b);
            *self.1 = end;
            Ok(())
        }
    }
    let _ = core::fmt::write(&mut FloatBuf(&mut buf, &mut pos), format_args!("{v}"));
    // SAFETY: `FloatBuf::write_str` copies bytes from `&str` arguments only,
    // so every byte written is valid UTF-8.
    ser.number_raw(&buf[..pos])
}

impl Serialize for f32 {
    fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        serialize_float(ser, *self as f64)
    }
}

impl Serialize for f64 {
    fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        serialize_float(ser, *self)
    }
}

#[cfg(feature = "alloc")]
impl<T: Serialize> Serialize for alloc::vec::Vec<T> {
    fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        self.as_slice().serialize(ser)
    }
}

#[cfg(feature = "alloc")]
impl<T: Serialize> Serialize for alloc::boxed::Box<T> {
    fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        (**self).serialize(ser)
    }
}

macro_rules! impl_serialize_map {
    ($($bound:tt)*) => {
        fn serialize<__W: Write>(&self, ser: &mut Serializer<__W>) -> Result<(), SerializeError<__W::Error>> {
            ser.object_begin()?;
            for (k, v) in self {
                ser.member_key(k.as_ref())?;
                v.serialize(ser)?;
            }
            ser.object_end()
        }
    };
}

#[cfg(feature = "alloc")]
impl<K: AsRef<str>, V: Serialize> Serialize for alloc::collections::BTreeMap<K, V> {
    impl_serialize_map!();
}

#[cfg(feature = "std")]
impl<K: AsRef<str> + Eq + std::hash::Hash, V: Serialize> Serialize
    for std::collections::HashMap<K, V>
{
    impl_serialize_map!();
}

#[cfg(feature = "arrayvec")]
impl<T: Serialize, const N: usize> Serialize for arrayvec::ArrayVec<T, N> {
    fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        self.as_slice().serialize(ser)
    }
}

#[cfg(feature = "arrayvec")]
impl<const N: usize> Serialize for arrayvec::ArrayString<N> {
    fn serialize<W: Write>(&self, ser: &mut Serializer<W>) -> Result<(), SerializeError<W::Error>> {
        ser.string(self.as_str())
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
/// let mut buf = [0u8; 32];
/// let json = nanojson::stringify_manual_sized(&mut buf, |s| {
///     s.object_begin()?;
///     s.member_key("n")?; s.integer(7)?;
///     s.object_end()
/// }).unwrap();
/// assert_eq!(json, b"{\"n\":7}");
/// ```
#[inline]
pub fn stringify_manual_sized<'buf>(
    buf: &'buf mut [u8],
    f: impl FnOnce(&mut Serializer<&mut crate::write::SliceWriter<'_>>) -> Result<(), SerializeError<WriteError>>,
) -> Result<&'buf mut [u8], SerializeError<WriteError>> {
    let mut w = crate::write::SliceWriter::new(buf);
    let mut ser = Serializer::new(&mut w);
    f(&mut ser)?;
    let len = w.pos();
    Ok(&mut buf[..len])
}

/// Serialize a `T: Serialize` value into a stack-allocated `[u8; N]` buffer.
///
/// # Example
/// ```
/// let mut buf = [0u8; 32];
/// let json = nanojson::stringify_sized(&mut buf, &42i64).unwrap();
/// assert_eq!(json, b"42");
/// ```
#[inline]
pub fn stringify_sized<'buf, T: Serialize>(
    buf: &'buf mut [u8],
    val: &T,
) -> Result<&'buf mut [u8], SerializeError<WriteError>> {
    stringify_manual_sized(buf, |s| val.serialize(s))
}

/// Serialize via closure into a stack-allocated `[u8; N]` buffer with pretty-printing.
///
/// # Example
/// ```
/// let mut buf = [0u8; 64];
/// let json = nanojson::stringify_manual_sized_pretty(&mut buf, 2, |s| {
///     s.object_begin()?;
///     s.member_key("x")?; s.integer(1)?;
///     s.object_end()
/// }).unwrap();
/// assert_eq!(json, b"{\n  \"x\": 1\n}");
/// ```
#[inline]
pub fn stringify_manual_sized_pretty<'buf>(
    buf: &'buf mut [u8],
    indent: usize,
    f: impl FnOnce(&mut Serializer<&mut crate::write::SliceWriter<'_>>) -> Result<(), SerializeError<WriteError>>,
) -> Result<&'buf mut [u8], SerializeError<WriteError>> {
    let mut w = crate::write::SliceWriter::new(buf);
    let mut ser = Serializer::with_pp(&mut w, indent);
    f(&mut ser)?;
    let len = w.pos();
    Ok(&mut buf[..len])
}

/// Serialize a `T: Serialize` value into a stack-allocated `[u8; N]` buffer with pretty-printing.
///
/// # Example
/// ```
/// # use nanojson::Serialize;
/// # #[cfg(feature = "derive")] {
/// #[derive(nanojson::Serialize)]
/// struct Point { x: i64, y: i64 }
/// let mut buf = [0u8; 64];
/// let json = nanojson::stringify_sized_pretty(&mut buf, 2, &Point { x: 1, y: 2 }).unwrap();
/// assert_eq!(json, b"{\n  \"x\": 1,\n  \"y\": 2\n}");
/// # }
/// ```
#[inline]
pub fn stringify_sized_pretty<'buf, T: Serialize>(
    buf: &'buf mut [u8],
    indent: usize,
    val: &T,
) -> Result<&'buf mut [u8], SerializeError<WriteError>> {
    stringify_manual_sized_pretty(buf, indent, |s| val.serialize(s))
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
    std::string::String::from_utf8(vec).map_err(
        |e| SerializeError::InvalidUtf8(e.utf8_error().error_len().unwrap_or(0))
    )
}

/// Serialize a value into a pretty-printed heap-allocated [`String`].
/// Only fails if nesting exceeds the default depth limit (32).
///
/// # Example
/// ```
/// let json = nanojson::stringify_pretty(2, &[1i64, 2, 3]).unwrap();
/// assert_eq!(json, "[\n  1,\n  2,\n  3\n]");
/// ```
#[cfg(feature = "std")]
#[inline]
pub fn stringify_pretty<T: Serialize>(
    indent: usize,
    val: &T,
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
    std::string::String::from_utf8(vec).map_err(
        |e| SerializeError::InvalidUtf8(e.utf8_error().error_len().unwrap_or(0))
    )
}

#[cfg(feature = "std")]
#[test]
fn outputs_invalid_utf8_for_malformed_sequence() {
    // 0xE2 starts a 3-byte sequence, but 0x28 ('(') is not a continuation byte.
    // 0xA1 is a lone continuation byte.  Both non-ASCII bytes must be \uXXXX-escaped.
    let input = [0xE2u8, 0x28, 0xA1];

    let json = stringify_manual(|s| s.string_bytes(&input)).unwrap();
    let ans = r#""\u00e2(\u00a1""#;
    assert_eq!(json, ans);
    assert!(std::str::from_utf8(json.as_bytes()).is_ok(),
        "Output is not valid UTF-8: {:?}", json);
}

#[test]
fn escapes_vertical_tab_as_unicode() {
    // \v (0x0B) is not a valid JSON escape; must be emitted as \u000b.
    let mut out = [0u8; 16];
    let json = stringify_manual_sized(&mut out, |s| s.string_bytes(&[0x0B])).unwrap();
    assert_eq!(&json[..], r#""\u000b""#.as_bytes());
}
