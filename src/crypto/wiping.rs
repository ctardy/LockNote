// Buffers for plaintext that leave no copy behind when they grow (issue #42).
//
// A `Vec` or a `String` that runs out of room moves its bytes to a larger
// allocation and frees the old one as it is: every reallocation leaves a
// copy of the plaintext in freed memory. `WipingBuf` grows by hand instead:
// it copies the bytes into the larger allocation, wipes the old one (its
// whole capacity) and only then frees it. It wipes itself on drop, and
// `clear` wipes what it held.
//
// `WipingString` is the same buffer written only with `&str`, `char` and
// base64 (ASCII), so it always holds UTF-8: `into_string` hands it over as a
// `String` by a move, without a copy.
//
// Out of reach of these buffers: the internal state of the code that writes
// into them (the gzip encoder window, the inflater dictionary, the serde_json
// scratch buffer); see the modules that use them.

use base64::Engine;
use zeroize::Zeroize;

/// Bytes wiped before any allocation that held them is freed.
pub struct WipingBuf {
    buf: Vec<u8>,
}

impl WipingBuf {
    pub fn new() -> Self {
        WipingBuf { buf: Vec::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        WipingBuf { buf: Vec::with_capacity(capacity) }
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.buf.capacity()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    /// Wipe the content (the whole allocation); the allocation is kept.
    pub fn clear(&mut self) {
        self.buf.zeroize();
    }

    /// Make room for `additional` more bytes, growing as described at the top
    /// of this module (at least twice the old capacity).
    pub fn reserve(&mut self, additional: usize) {
        let needed = self.buf.len().checked_add(additional).expect("WipingBuf: capacity overflow");
        if needed > self.buf.capacity() {
            let new_capacity = needed.max(self.buf.capacity().saturating_mul(2)).max(64);
            drop(self.move_to_larger(new_capacity));
        }
    }

    /// Move the bytes into a new allocation of `new_capacity` and return the
    /// old one, wiped (length 0, capacity kept) so the caller frees only zeros.
    fn move_to_larger(&mut self, new_capacity: usize) -> Vec<u8> {
        let mut grown = Vec::with_capacity(new_capacity);
        grown.extend_from_slice(&self.buf);
        let mut old = std::mem::replace(&mut self.buf, grown);
        old.zeroize();
        old
    }

    pub fn extend_from_slice(&mut self, bytes: &[u8]) {
        self.reserve(bytes.len());
        self.buf.extend_from_slice(bytes);
    }

    /// The bytes, moved out (no copy): the caller wipes them from here on.
    pub fn into_vec(mut self) -> Vec<u8> {
        std::mem::take(&mut self.buf)
    }
}

impl Default for WipingBuf {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for WipingBuf {
    fn drop(&mut self) {
        self.buf.zeroize();
    }
}

impl std::io::Write for WipingBuf {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Text in a `WipingBuf`: only UTF-8 is ever written into it.
#[derive(Default)]
pub struct WipingString(WipingBuf);

impl WipingString {
    pub fn new() -> Self {
        WipingString(WipingBuf::new())
    }

    pub fn with_capacity(capacity: usize) -> Self {
        WipingString(WipingBuf::with_capacity(capacity))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(self.0.as_bytes()).expect("WipingString: only UTF-8 is written")
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn push_str(&mut self, s: &str) {
        self.0.extend_from_slice(s.as_bytes());
    }

    pub fn push(&mut self, c: char) {
        self.push_str(c.encode_utf8(&mut [0u8; 4]));
    }

    /// Append the standard base64 (padded) of `data`, encoded straight into
    /// the buffer: no intermediate `String` and no chunk buffer
    /// (`Engine::encode_string` goes through one on the stack).
    pub fn push_base64(&mut self, data: &[u8]) {
        let encoded = base64::encoded_len(data.len(), true).expect("WipingString: base64 length overflow");
        self.0.reserve(encoded);
        let start = self.0.buf.len();
        self.0.buf.resize(start + encoded, 0);
        let written = base64::engine::general_purpose::STANDARD
            .encode_slice(data, &mut self.0.buf[start..])
            .expect("WipingString: the slice is sized by encoded_len");
        debug_assert_eq!(written, encoded);
    }

    /// The text, moved out as a `String` (no copy): the caller wipes it from
    /// here on.
    pub fn into_string(self) -> String {
        String::from_utf8(self.0.into_vec()).expect("WipingString: only UTF-8 is written")
    }
}

impl std::fmt::Write for WipingString {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.push_str(s);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every byte of the allocation of `v`, spare capacity included. Only for
    /// a `Vec` whose whole capacity was written (zeroize writes all of it).
    fn whole_allocation(v: &Vec<u8>) -> &[u8] {
        // SAFETY: `v` is alive and owns `capacity()` bytes; zeroize wrote every
        // one of them, so they are initialized.
        unsafe { std::slice::from_raw_parts(v.as_ptr(), v.capacity()) }
    }

    #[test]
    fn growth_wipes_the_old_allocation_before_it_is_freed() {
        let mut b = WipingBuf::with_capacity(8);
        b.extend_from_slice(b"secret!!");
        let old_ptr = b.as_bytes().as_ptr();
        let old = b.move_to_larger(64);
        assert_eq!(old.as_ptr(), old_ptr, "the old allocation is handed back");
        assert_eq!((old.len(), old.capacity()), (0, 8));
        assert_eq!(whole_allocation(&old), &[0u8; 8]);
        assert_eq!(b.as_bytes(), b"secret!!");
        assert_eq!(b.capacity(), 64);
    }

    #[test]
    fn reserve_grows_only_when_needed_and_at_least_doubles() {
        let mut b = WipingBuf::with_capacity(100);
        b.extend_from_slice(&[1u8; 100]);
        let ptr = b.as_bytes().as_ptr();
        b.reserve(0);
        assert_eq!((b.capacity(), b.as_bytes().as_ptr()), (100, ptr), "no growth when it fits");
        b.extend_from_slice(&[2u8; 1]);
        assert_eq!(b.capacity(), 200);
        b.extend_from_slice(&[3u8; 500]);
        assert_eq!(b.capacity(), 601);
        let mut want = vec![1u8; 100];
        want.push(2);
        want.extend_from_slice(&[3u8; 500]);
        assert_eq!(b.as_bytes(), &want[..]);
        let mut e = WipingBuf::new();
        e.extend_from_slice(b"x");
        assert_eq!(e.capacity(), 64);
    }

    #[test]
    fn clear_wipes_the_whole_allocation() {
        let mut b = WipingBuf::with_capacity(32);
        b.extend_from_slice(b"0123456789");
        b.clear();
        assert!(b.is_empty());
        assert_eq!(whole_allocation(&b.buf), &[0u8; 32]);
    }

    #[test]
    fn writes_through_io_write() {
        use std::io::Write;
        let mut b = WipingBuf::new();
        b.write_all(b"ab").unwrap();
        write!(b, "{}", 12).unwrap();
        assert_eq!(b.as_bytes(), b"ab12");
    }

    #[test]
    fn into_vec_and_into_string_move_without_copying() {
        let mut b = WipingBuf::with_capacity(16);
        b.extend_from_slice(b"abc");
        let ptr = b.as_bytes().as_ptr();
        let v = b.into_vec();
        assert_eq!((v.as_ptr(), &v[..], v.capacity()), (ptr, &b"abc"[..], 16));

        let mut s = WipingString::with_capacity(16);
        s.push_str("d\u{e9}");
        s.push('\u{1f512}');
        let ptr = s.as_str().as_ptr();
        let t = s.into_string();
        assert_eq!((t.as_ptr(), t.as_str()), (ptr, "d\u{e9}\u{1f512}"));
    }

    #[test]
    fn push_base64_is_the_standard_encoding_written_in_place() {
        for data in [&b""[..], b"f", b"fo", b"foo", b"foob", &[0u8, 255, 128, 7, 9][..]] {
            let mut s = WipingString::new();
            s.push_str("x:");
            s.push_base64(data);
            assert_eq!(s.as_str(), format!("x:{}", base64::engine::general_purpose::STANDARD.encode(data)));
        }
        // Sized up front, it fills the buffer exactly: no growth.
        let data = vec![42u8; 3000];
        let mut s = WipingString::with_capacity(base64::encoded_len(data.len(), true).unwrap());
        s.push_base64(&data);
        assert_eq!(s.len(), s.capacity());
    }

    #[test]
    fn writes_through_fmt_write() {
        use std::fmt::Write;
        let mut s = WipingString::new();
        write!(s, "{}|{}", "a", 3).unwrap();
        assert_eq!(s.as_str(), "a|3");
        s.clear();
        s.push_str("b");
        assert_eq!(s.as_str(), "b");
    }

    /// Wiping on drop cannot be watched (the memory is freed): pinned here.
    #[test]
    fn drop_and_growth_wipe() {
        let src = crate::read_source("src/crypto/wiping.rs");
        let drop_impl = &src[src.find("impl Drop for WipingBuf {").unwrap()..];
        let drop_impl = &drop_impl[..drop_impl.find("\n}\n").unwrap()];
        assert!(drop_impl.contains("self.buf.zeroize();"), "{drop_impl}");
        let reserve = &src[src.find("    pub fn reserve(").unwrap()..];
        let reserve = &reserve[..reserve.find("\n    }\n").unwrap()];
        assert!(reserve.contains("drop(self.move_to_larger(new_capacity));"), "{reserve}");
        assert!(!reserve.contains("self.buf.reserve"), "{reserve}");
        let b64 = &src[src.find("    pub fn push_base64(").unwrap()..];
        let b64 = &b64[..b64.find("
    }
").unwrap()];
        assert!(b64.contains(".encode_slice(data, &mut self.0.buf[start..])"), "{b64}");
        assert!(!b64.contains(".encode(") && !b64.contains("encode_string"), "{b64}");
    }
}
