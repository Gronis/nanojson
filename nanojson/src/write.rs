use crate::WriteError;

/// Trait for byte sinks used by the serializer.
/// Implement this to target any output: a slice, a ring buffer, a UART, etc.
pub trait Write {
    type Error;
    fn write_bytes(&mut self, b: &[u8]) -> Result<(), Self::Error>;
}

/// Writes into a user-supplied byte slice.
pub struct SliceWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> SliceWriter<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self { Self { buf, pos: 0 } }
    /// Returns the bytes written so far.
    pub fn written(&self) -> &[u8] { &self.buf[..self.pos] }
    /// Returns the current write position.
    pub fn pos(&self) -> usize { self.pos }
    /// Reset the write position to 0.
    pub fn reset(&mut self) { self.pos = 0 }
}

impl Write for SliceWriter<'_> {
    type Error = WriteError;

    fn write_bytes(&mut self, b: &[u8]) -> Result<(), WriteError> {
        let end = self.pos + b.len();
        if end > self.buf.len() { return Err(WriteError::BufferFull) };
        self.buf[self.pos..end].copy_from_slice(b);
        self.pos = end;
        Ok(())
    }
}

/// A write sink that counts bytes without storing them.
/// Useful for pre-computing the required buffer size.
#[derive(Default)]
pub struct SizeCounter {
    /// Running byte count.
    ///
    /// On 32-bit targets this silently wraps if total output exceeds 4 GiB;
    /// this is not a concern in practice for embedded use.
    pub count: usize,
}

impl SizeCounter {
    pub fn new() -> Self { Self { count: 0 } }
}

impl<W: Write> Write for &mut W {
    type Error = W::Error;
    fn write_bytes(&mut self, b: &[u8]) -> Result<(), W::Error> {
        (*self).write_bytes(b)
    }
}

impl Write for SizeCounter {
    type Error = core::convert::Infallible;

    fn write_bytes(&mut self, b: &[u8]) -> Result<(), core::convert::Infallible> {
        self.count += b.len();
        Ok(())
    }
}

#[cfg(feature = "std")]
impl Write for std::vec::Vec<u8> {
    type Error = core::convert::Infallible;

    fn write_bytes(&mut self, b: &[u8]) -> Result<(), core::convert::Infallible> {
        self.extend_from_slice(b);
        Ok(())
    }
}
