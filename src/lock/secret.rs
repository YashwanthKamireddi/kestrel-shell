//! The typed password: one fixed buffer, wiped whenever it is cleared, taken or dropped.
//!
//! The buffer's room is reserved once and never grows, so no reallocation can leave a copy of
//! the password behind in freed memory. A character that would not fit is refused (PAM takes
//! no answer longer than `PAM_MAX_RESP_SIZE`, 512 bytes, either). It is never printed: its
//! `Debug` shows nothing of it.

use zeroize::{Zeroize, Zeroizing};

/// Room for the password, in bytes of UTF-8 (`PAM_MAX_RESP_SIZE` in Linux-PAM's
/// `_pam_types.h`, less the terminating NUL).
pub const CAPACITY: usize = 511;

pub struct Secret(Zeroizing<Vec<u8>>);

impl Secret {
    pub fn new() -> Self {
        Self(Zeroizing::new(Vec::with_capacity(CAPACITY)))
    }

    /// Adds a character; false (and nothing added) when it would not fit or is a control
    /// character.
    pub fn push(&mut self, c: char) -> bool {
        if c.is_control() {
            return false;
        }
        let mut buf = [0u8; 4];
        let bytes = c.encode_utf8(&mut buf).as_bytes();
        let fits = self.0.len() + bytes.len() <= CAPACITY;
        if fits {
            self.0.extend_from_slice(bytes);
        }
        buf.zeroize();
        fits
    }

    /// Removes the last character, wiping its bytes.
    pub fn pop(&mut self) {
        let Some(start) = self.0.iter().rposition(|b| b & 0b1100_0000 != 0b1000_0000) else {
            return;
        };
        self.0[start..].zeroize();
        self.0.truncate(start);
    }

    /// Wipes everything typed.
    pub fn clear(&mut self) {
        // Zeroes every byte of the buffer (spare room too) and empties it; the room stays.
        self.0.zeroize();
    }

    /// Moves the password out into a new buffer and wipes this one.
    pub fn take(&mut self) -> Secret {
        let mut out = Secret::new();
        out.0.extend_from_slice(&self.0);
        self.clear();
        out
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// How many characters were typed (what the dots show).
    pub fn chars(&self) -> usize {
        self.0
            .iter()
            .filter(|b| *b & 0b1100_0000 != 0b1000_0000)
            .count()
    }

    /// The password's bytes (UTF-8, no NUL), to hand to PAM.
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

impl Default for Secret {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(s: &str) -> Secret {
        let mut p = Secret::new();
        for c in s.chars() {
            assert!(p.push(c));
        }
        p
    }

    #[test]
    fn types_and_deletes_whole_characters() {
        let mut p = typed("aé€😀");
        assert_eq!(p.chars(), 4);
        p.pop();
        assert_eq!(p.bytes(), "aé€".as_bytes());
        p.pop();
        p.pop();
        assert_eq!(p.bytes(), b"a");
        p.pop();
        p.pop();
        assert!(p.is_empty());
    }

    #[test]
    fn never_grows_past_its_room() {
        let mut p = Secret::new();
        let ptr = p.0.as_ptr();
        while p.push('x') {}
        assert_eq!(p.bytes().len(), CAPACITY);
        assert!(!p.push('y'));
        // Same allocation throughout: nothing was copied elsewhere.
        assert_eq!(p.0.as_ptr(), ptr);
    }

    #[test]
    fn refuses_control_characters() {
        let mut p = Secret::new();
        assert!(!p.push('\n'));
        assert!(!p.push('\u{8}'));
        assert!(p.is_empty());
    }

    #[test]
    fn take_moves_and_wipes() {
        let mut p = typed("hunter2");
        let ptr = p.0.as_ptr();
        let out = p.take();
        assert_eq!(out.bytes(), b"hunter2");
        assert!(p.is_empty());
        assert!(p.0.capacity() >= CAPACITY);
        // The old buffer's bytes are zero (read through the spare room).
        // Safety: `ptr` is p's live allocation of at least 7 bytes; reading u8s is valid.
        let old = unsafe { std::slice::from_raw_parts(ptr, 7) };
        assert_eq!(old, &[0u8; 7]);
    }

    #[test]
    fn pop_wipes_the_removed_bytes() {
        let mut p = typed("ab€");
        let ptr = p.0.as_ptr();
        p.pop();
        // Safety: as above, within the allocation.
        let raw = unsafe { std::slice::from_raw_parts(ptr, 5) };
        assert_eq!(raw, b"ab\0\0\0");
    }

    #[test]
    fn debug_shows_nothing() {
        let p = typed("secret");
        assert_eq!(format!("{p:?}"), "Secret(..)");
    }
}
