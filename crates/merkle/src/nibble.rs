//! `Nibble` represents a four-bit unsigned integer.

use serde::{Deserialize, Serialize};
use std::fmt;

// /// The hardcoded maximum height of a state merkle tree in nibbles.
// pub const ROOT_NIBBLE_HEIGHT: usize = HashValue::LENGTH * 2;

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Nibble(u8);

impl From<u8> for Nibble {
    fn from(nibble: u8) -> Self {
        assert!(nibble < 16, "Nibble out of range: {}", nibble);
        Self(nibble)
    }
}

impl From<Nibble> for u8 {
    fn from(nibble: Nibble) -> Self {
        nibble.0
    }
}

impl fmt::LowerHex for Nibble {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:x}", self.0)
    }
}

/// NibblePath defines a path in a Merkle tree in the unit of nibble (4 bits).
#[derive(Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct NibblePath {
    /// Indicates the total number of nibbles in bytes. Either `bytes.len() * 2 - 1` or
    /// `bytes.len() * 2`.
    // Guarantees intended ordering based on the top-to-bottom declaration order of the struct's
    // members.
    len: u8,
    /// The underlying bytes that stores the path, 2 nibbles per byte. If the number of nibbles is
    /// odd, the second half of the last byte must be 0.
    bytes: [u8; Self::MAX_BYTE_PATH_LENGTH],
    // invariant num_nibbles <= ROOT_NIBBLE_HEIGHT
}

/// Supports debug format by concatenating nibbles literally. For example, [0x12, 0xa0] with 3
/// nibbles will be printed as "12a".
impl fmt::Debug for NibblePath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.nibbles().try_for_each(|x| write!(f, "{:x}", x))
    }
}

/// Convert a vector of bytes into `NibblePath` using the lower 4 bits of each byte as nibble.
impl FromIterator<Nibble> for NibblePath {
    fn from_iter<I: IntoIterator<Item = Nibble>>(iter: I) -> Self {
        let mut nibble_path = NibblePath::new_even(vec![]);
        for nibble in iter {
            nibble_path.push(nibble);
        }
        nibble_path
    }
}

impl NibblePath {
    const MAX_BYTE_PATH_LENGTH: usize = 32;
    const MAX_NIBBLE_PATH_LENGTH: usize = Self::MAX_BYTE_PATH_LENGTH * 2;

    /// Creates a new `NibblePath` from a vector of bytes assuming each byte has 2 nibbles.
    pub fn new_even<T: AsRef<[u8]>>(bytes: T) -> Self {
        let bytes = bytes.as_ref();
        assert!(bytes.len() <= Self::MAX_BYTE_PATH_LENGTH);
        let num_nibbles = bytes.len() * 2;

        let mut buf = [0; Self::MAX_BYTE_PATH_LENGTH];
        buf[..bytes.len()].copy_from_slice(bytes);

        Self {
            len: num_nibbles.try_into().unwrap(),
            bytes: buf,
        }
    }

    /// Similar to `new()` but asserts that the bytes have one less nibble.
    pub fn new_odd<T: AsRef<[u8]>>(bytes: T) -> Self {
        let bytes = bytes.as_ref();

        assert!(bytes.len() <= Self::MAX_BYTE_PATH_LENGTH);
        assert_eq!(bytes.last().unwrap() & 0x0F, 0, "Last nibble must be 0.");

        let num_nibbles = bytes.len() * 2 - 1;

        let mut buf = [0; Self::MAX_BYTE_PATH_LENGTH];
        buf[..bytes.len()].copy_from_slice(bytes);

        Self {
            len: num_nibbles.try_into().unwrap(),
            bytes: buf,
        }
    }

    /// Explicitly grab the number of nibbles from `bytes`, if num_nibbles is odd then the final
    /// nibble is zero'd out for the user
    fn new_from_byte_array(bytes: &[u8], num_nibbles: usize) -> Self {
        assert!(num_nibbles <= Self::MAX_NIBBLE_PATH_LENGTH);

        if num_nibbles % 2 == 1 {
            // Rounded up number of bytes to be considered
            let num_bytes = (num_nibbles + 1) / 2;
            assert!(bytes.len() >= num_bytes);

            let mut buf = [0; Self::MAX_BYTE_PATH_LENGTH];
            buf[..num_bytes].copy_from_slice(bytes);
            // make sure to pad the last nibble with 0s.
            let last_byte_padded = bytes[num_bytes - 1] & 0xF0;
            buf[num_bytes - 1] = last_byte_padded;

            Self {
                len: num_nibbles.try_into().unwrap(),
                bytes: buf,
            }
        } else {
            assert!(bytes.len() >= num_nibbles / 2);
            NibblePath::new_even(&bytes[..num_nibbles / 2])
        }
    }

    /// Get the total number of nibbles stored.
    pub fn len(&self) -> usize {
        self.len as usize
    }

    ///  Returns `true` if the nibbles contains no elements.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Adds a nibble to the end of the nibble path.
    pub fn push(&mut self, nibble: Nibble) {
        assert!(Self::MAX_NIBBLE_PATH_LENGTH > self.len());

        self.set_internal(nibble, self.len());
        self.len += 1;
    }

    fn set_internal(&mut self, nibble: Nibble, nibble_idx: usize) {
        let current_byte = self.bytes[nibble_idx / 2];

        let is_high_bits = nibble_idx % 2 == 0;

        // If we are setting high bits then we want to preserve the lower bits and clear out the
        // higher ones, otherwise we need to do the opposite
        let byte = current_byte & (if is_high_bits { 0x0F } else { 0xF0 })
            | u8::from(nibble) << (if is_high_bits { 4 } else { 0 });

        self.bytes[nibble_idx / 2] = byte;
    }

    fn get_internal(&self, nibble_idx: usize) -> Nibble {
        Nibble::from(
            (self.bytes[nibble_idx / 2] >> (if nibble_idx % 2 == 0 { 4 } else { 0 })) & 0xF,
        )
    }

    /// Get the i-th nibble.
    pub fn get_nibble(&self, i: usize) -> Nibble {
        assert!(i < self.len());
        self.get_internal(i)
    }

    /// Pops a nibble from the end of the nibble path.
    pub fn pop(&mut self) -> Option<Nibble> {
        if self.is_empty() {
            return None;
        }

        let nibble = self.get_nibble(self.len() - 1);
        self.set_internal(Nibble::from(0), self.len() - 1);
        self.len -= 1;

        Some(nibble)
    }

    /// Returns the last nibble.
    pub fn last(&self) -> Option<Nibble> {
        if self.is_empty() {
            return None;
        }

        let nibble = self.get_nibble(self.len() - 1);
        Some(nibble)
    }

    /// Get the i-th bit.
    fn get_bit(&self, i: usize) -> bool {
        assert!(i < self.num_nibbles * 4);
        let pos = i / 8;
        let bit = 7 - i % 8;
        ((self.bytes[pos] >> bit) & 1) != 0
    }

    /// Get a bit iterator iterates over the whole nibble path.
    pub fn bits(&self) -> BitIterator {
        BitIterator {
            nibble_path: self,
            pos: (0..self.num_nibbles * 4),
        }
    }

    /// Get a nibble iterator iterates over the whole nibble path.
    pub fn nibbles(&self) -> NibbleIterator {
        NibbleIterator::new(self, 0, self.num_nibbles)
    }

    /// Get the underlying bytes storing nibbles.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn truncate(&mut self, len: usize) {
        assert!(len <= self.num_nibbles);
        self.num_nibbles = len;
        self.bytes.truncate((len + 1) / 2);
        if len % 2 != 0 {
            *self.bytes.last_mut().expect("must exist.") &= 0xF0;
        }
    }

    // Returns the shard_id of the NibblePath, or None if it is root.
    pub fn get_shard_id(&self) -> Option<u8> {
        if self.num_nibbles() > 0 {
            Some(u8::from(self.get_nibble(0)))
        } else {
            None
        }
    }
}

pub trait Peekable: Iterator {
    /// Returns the `next()` value without advancing the iterator.
    fn peek(&self) -> Option<Self::Item>;
}

/// BitIterator iterates a nibble path by bit.
pub struct BitIterator<'a> {
    nibble_path: &'a NibblePath,
    pos: std::ops::Range<usize>,
}

impl<'a> Peekable for BitIterator<'a> {
    /// Returns the `next()` value without advancing the iterator.
    fn peek(&self) -> Option<Self::Item> {
        if self.pos.start < self.pos.end {
            Some(self.nibble_path.get_bit(self.pos.start))
        } else {
            None
        }
    }
}

/// BitIterator spits out a boolean each time. True/false denotes 1/0.
impl<'a> Iterator for BitIterator<'a> {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        self.pos.next().map(|i| self.nibble_path.get_bit(i))
    }
}

/// Support iterating bits in reversed order.
impl<'a> DoubleEndedIterator for BitIterator<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.pos.next_back().map(|i| self.nibble_path.get_bit(i))
    }
}

/// NibbleIterator iterates a nibble path by nibble.
#[derive(Debug)]
pub struct NibbleIterator<'a> {
    /// The underlying nibble path that stores the nibbles
    nibble_path: &'a NibblePath,

    /// The current index, `pos.start`, will bump by 1 after calling `next()` until `pos.start ==
    /// pos.end`.
    pos: std::ops::Range<usize>,

    /// The start index of the iterator. At the beginning, `pos.start == start`. [start, pos.end)
    /// defines the range of `nibble_path` this iterator iterates over. `nibble_path` refers to
    /// the entire underlying buffer but the range may only be partial.
    start: usize,
    // invariant self.start <= self.pos.start;
    // invariant self.pos.start <= self.pos.end;
    // invariant self.pos.end <= ROOT_NIBBLE_HEIGHT;
}

/// NibbleIterator spits out a byte each time. Each byte must be in range [0, 16).
impl<'a> Iterator for NibbleIterator<'a> {
    type Item = Nibble;

    fn next(&mut self) -> Option<Self::Item> {
        self.pos.next().map(|i| self.nibble_path.get_nibble(i))
    }
}

impl<'a> Peekable for NibbleIterator<'a> {
    /// Returns the `next()` value without advancing the iterator.
    fn peek(&self) -> Option<Self::Item> {
        if self.pos.start < self.pos.end {
            Some(self.nibble_path.get_nibble(self.pos.start))
        } else {
            None
        }
    }
}

impl<'a> NibbleIterator<'a> {
    fn new(nibble_path: &'a NibblePath, start: usize, end: usize) -> Self {
        assert!(start <= end);
        assert!(start <= ROOT_NIBBLE_HEIGHT);
        assert!(end <= ROOT_NIBBLE_HEIGHT);
        Self {
            nibble_path,
            pos: (start..end),
            start,
        }
    }

    /// Returns a nibble iterator that iterates all visited nibbles.
    pub fn visited_nibbles(&self) -> NibbleIterator<'a> {
        Self::new(self.nibble_path, self.start, self.pos.start)
    }

    /// Returns a nibble iterator that iterates all remaining nibbles.
    pub fn remaining_nibbles(&self) -> NibbleIterator<'a> {
        Self::new(self.nibble_path, self.pos.start, self.pos.end)
    }

    /// Turn it into a `BitIterator`.
    pub fn bits(&self) -> BitIterator<'a> {
        BitIterator {
            nibble_path: self.nibble_path,
            pos: (self.pos.start * 4..self.pos.end * 4),
        }
    }

    /// Cut and return the range of the underlying `nibble_path` that this iterator is iterating
    /// over as a new `NibblePath`
    pub fn get_nibble_path(&self) -> NibblePath {
        self.visited_nibbles()
            .chain(self.remaining_nibbles())
            .collect()
    }

    /// Get the number of nibbles that this iterator covers.
    pub fn num_nibbles(&self) -> usize {
        assert!(self.start <= self.pos.end); // invariant
        self.pos.end - self.start
    }

    /// Return `true` if the iteration is over.
    pub fn is_finished(&self) -> bool {
        self.peek().is_none()
    }
}

/// Advance both iterators if their next nibbles are the same until either reaches the end or
/// the find a mismatch. Return the number of matched nibbles.
pub fn skip_common_prefix<I1, I2>(x: &mut I1, y: &mut I2) -> usize
where
    I1: Iterator + Peekable,
    I2: Iterator + Peekable,
    <I1 as Iterator>::Item: std::cmp::PartialEq<<I2 as Iterator>::Item>,
{
    let mut count = 0;
    loop {
        let x_peek = x.peek();
        let y_peek = y.peek();
        if x_peek.is_none()
            || y_peek.is_none()
            || x_peek.expect("cannot be none") != y_peek.expect("cannot be none")
        {
            break;
        }
        count += 1;
        x.next();
        y.next();
    }
    count
}
