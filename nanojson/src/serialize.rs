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
/// ```ignore
/// let mut buf = [0u8; 256];
/// let mut w = SliceWriter::new(&mut buf);
/// let mut ser = Serializer::new(&mut w);
/// ser.object_begin()?;
///   ser.member_key("x")?; ser.integer(1)?;
/// ser.object_end()?;
/// ```
pub struct Serializer<W, const DEPTH: usize = 32> {
    writer: W,
    scopes: [Scope; DEPTH],
    depth: usize,
    /// Pretty-print indent width in spaces. 0 = compact output.
    pub pp: usize,
}

/// Error type for the serializer: either a write error from the sink, or nesting depth exceeded.
#[derive(Debug)]
pub enum SerializeError<E> {
    Write(E),
    DepthExceeded,
}

impl<E> From<E> for SerializeError<E> {
    fn from(e: E) -> Self {
        SerializeError::Write(e)
    }
}

// Convenience alias when using SliceWriter
impl From<SerializeError<WriteError>> for WriteError {
    fn from(e: SerializeError<WriteError>) -> Self {
        match e {
            SerializeError::Write(e) => e,
            SerializeError::DepthExceeded => WriteError::DepthExceeded,
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
        let scope = self.current_scope().expect("member_key called outside object");
        debug_assert_eq!(scope.kind, ScopeKind::Object);
        debug_assert!(!scope.key, "member_key called twice without a value");
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
#[inline]
pub fn serialize<const N: usize>(
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
#[inline]
pub fn to_json<const N: usize, T: Serialize>(
    val: &T,
) -> Result<([u8; N], usize), SerializeError<WriteError>> {
    serialize::<N>(|s| val.serialize(s))
}

/// Count the bytes that a closure would produce without writing anything.
/// Returns the byte count; returns 0 if `DepthExceeded` is hit.
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
#[cfg(feature = "std")]
#[inline]
pub fn to_string<T: Serialize>(
    val: &T,
) -> Result<std::string::String, SerializeError<core::convert::Infallible>> {
    serialize_to_string(|s| val.serialize(s))
}

/// Serialize via closure into a heap-allocated [`String`].
/// The output buffer grows as needed; no size choice required.
/// Only fails if nesting exceeds the default depth limit (32).
#[cfg(feature = "std")]
#[inline]
pub fn serialize_to_string(
    f: impl FnOnce(&mut Serializer<std::vec::Vec<u8>>) -> Result<(), SerializeError<core::convert::Infallible>>,
) -> Result<std::string::String, SerializeError<core::convert::Infallible>> {
    let mut ser: Serializer<_> = Serializer::new(std::vec::Vec::new());
    f(&mut ser)?;
    let vec = ser.into_writer();
    // SAFETY: the serializer only writes valid JSON, which is always valid UTF-8.
    Ok(unsafe { std::string::String::from_utf8_unchecked(vec) })
}
