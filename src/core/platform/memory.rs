//! Buffers para estructuras que devuelven las APIs de Windows.

/// Buffer de bytes alineado a 8. Las APIs de Windows devuelven estructuras dentro de un buffer de bytes;
/// reinterpretar un `Vec<u8>` (alineado a 1) como esa estructura es comportamiento indefinido (SEC-28).
pub struct AlignedBuf {
    words: Vec<u64>,
    len: usize,
}

impl AlignedBuf {
    pub fn zeroed(len: usize) -> Self {
        Self {
            words: vec![0u64; len.div_ceil(8).max(1)],
            len,
        }
    }
}

impl std::ops::Deref for AlignedBuf {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.words.as_ptr() as *const u8, self.len) }
    }
}

impl std::ops::DerefMut for AlignedBuf {
    fn deref_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.words.as_mut_ptr() as *mut u8, self.len) }
    }
}
