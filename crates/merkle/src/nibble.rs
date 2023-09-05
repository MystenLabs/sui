//! `Nibble` represents a four-bit unsigned integer.

use proptest::{collection::vec, prelude::*};
use serde::{Deserialize, Serialize};
use std::{fmt, hash::Hash};

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Nibble(u8);

impl Nibble {
    pub fn inner(self) -> u8 {
        self.0
    }
}

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

impl Arbitrary for Nibble {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        (0..16u8).prop_map(Self::from).boxed()
    }
}

/// NibblePath defines a path in a Merkle tree in the unit of nibble (4 bits).
#[derive(Clone, Serialize, Deserialize)]
pub struct NibblePath {
    /// Indicates the total number of nibbles in bytes. Either `bytes.len() * 2 - 1` or
    /// `bytes.len() * 2`.
    // Guarantees intended ordering based on the top-to-bottom declaration order of the struct's
    // members.
    len: u8,
    /// The underlying bytes that stores the path, 2 nibbles per byte. If the number of nibbles is
    /// odd, the second half of the last byte must be 0.
    bytes: [u8; Self::MAX_BYTE_PATH_LENGTH],
    // invariant len <= Self::MAX_NIBBLE_PATH_LENGTH
}

impl Eq for NibblePath {}

impl PartialEq for NibblePath {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.bytes() == other.bytes()
    }
}

impl PartialOrd for NibblePath {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NibblePath {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.len.cmp(&other.len) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        self.bytes().cmp(other.bytes())
    }
}

impl Hash for NibblePath {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.len.hash(state);
        self.bytes().hash(state);
    }
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

    pub fn empty() -> Self {
        Self {
            len: 0,
            bytes: [0; Self::MAX_BYTE_PATH_LENGTH],
        }
    }

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
        assert_eq!(
            bytes.last().expect("Should have odd number of nibbles.") & 0x0F,
            0,
            "Last nibble must be 0."
        );

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
            buf[..num_bytes].copy_from_slice(&bytes[..num_bytes]);
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
        assert!(i < self.len() * 4);
        let pos = i / 8;
        let bit = 7 - i % 8;
        ((self.bytes[pos] >> bit) & 1) != 0
    }

    /// Get a bit iterator iterates over the whole nibble path.
    pub fn bits(&self) -> BitIterator {
        BitIterator {
            nibble_path: self,
            pos: (0..self.len() * 4),
        }
    }

    /// Get a nibble iterator iterates over the whole nibble path.
    pub fn nibbles(&self) -> NibbleIterator {
        NibbleIterator::new(self, 0, self.len())
    }

    /// Get the underlying bytes storing nibbles.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes[..((self.len() + 1) / 2)]
    }

    pub fn truncate(&mut self, len: usize) {
        assert!(len <= self.len());
        self.len = len as u8;
        for i in len..Self::MAX_NIBBLE_PATH_LENGTH {
            self.set_internal(Nibble::from(0), i);
        }
    }

    // Returns the shard_id of the NibblePath, or None if it is root.
    pub fn get_shard_id(&self) -> Option<u8> {
        if self.len() > 0 {
            Some(u8::from(self.get_nibble(0)))
        } else {
            None
        }
    }
}

impl Arbitrary for NibblePath {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        arb_nibble_path().boxed()
    }
}

prop_compose! {
    fn arb_nibble_path()(
        mut bytes in vec(any::<u8>(), 0..=NibblePath::MAX_BYTE_PATH_LENGTH),
        is_odd in any::<bool>()
    ) -> NibblePath {
        if let Some(last_byte) = bytes.last_mut() {
            if is_odd {
                *last_byte &= 0xf0;
                return NibblePath::new_odd(bytes);
            }
        }
        NibblePath::new_even(bytes)
    }
}

prop_compose! {
    fn arb_internal_nibble_path()(
        nibble_path in arb_nibble_path().prop_filter(
            "Filter out leaf paths.",
            |p| p.len() < NibblePath::MAX_NIBBLE_PATH_LENGTH,
        )
    ) -> NibblePath {
        nibble_path
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
    // invariant self.pos.end <= NibblePath::MAX_NIBBLE_PATH_LENGTH
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
        assert!(start <= NibblePath::MAX_NIBBLE_PATH_LENGTH);
        assert!(end <= NibblePath::MAX_NIBBLE_PATH_LENGTH);
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

    pub fn visited_nibble_path(&self) -> NibblePath {
        self.visited_nibbles().collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nibble_path_fmt() {
        let nibble_path = NibblePath::new_even(vec![0x12, 0x34, 0x56]);
        assert_eq!(format!("{:?}", nibble_path), "123456");

        let nibble_path = NibblePath::new_even(vec![0x12, 0x34, 0x50]);
        assert_eq!(format!("{:?}", nibble_path), "123450");

        let nibble_path = NibblePath::new_odd(vec![0x12, 0x34, 0x50]);
        assert_eq!(format!("{:?}", nibble_path), "12345");
    }

    #[test]
    fn test_create_nibble_path_success() {
        let nibble_path = NibblePath::new_even(vec![0x12, 0x34, 0x56]);
        assert_eq!(nibble_path.len(), 6);

        let nibble_path = NibblePath::new_even(vec![0x12, 0x34, 0x50]);
        assert_eq!(nibble_path.len(), 6);

        let nibble_path = NibblePath::new_odd(vec![0x12, 0x34, 0x50]);
        assert_eq!(nibble_path.len(), 5);

        let nibble_path = NibblePath::new_even(vec![]);
        assert_eq!(nibble_path.len(), 0);
    }

    #[test]
    #[should_panic(expected = "Last nibble must be 0.")]
    fn test_create_nibble_path_failure() {
        let bytes = vec![0x12, 0x34, 0x56];
        let _nibble_path = NibblePath::new_odd(bytes);
    }

    #[test]
    #[should_panic(expected = "Should have odd number of nibbles.")]
    fn test_empty_nibble_path() {
        NibblePath::new_odd(vec![]);
    }

    #[test]
    fn test_get_nibble() {
        let bytes = vec![0x12, 0x34];
        let nibble_path = NibblePath::new_even(bytes);
        assert_eq!(nibble_path.get_nibble(0), Nibble::from(0x01));
        assert_eq!(nibble_path.get_nibble(1), Nibble::from(0x02));
        assert_eq!(nibble_path.get_nibble(2), Nibble::from(0x03));
        assert_eq!(nibble_path.get_nibble(3), Nibble::from(0x04));
    }

    #[test]
    fn test_get_nibble_from_byte_array() {
        let bytes = vec![0x12, 0x34, 0x56, 0x78];
        let nibble_path = NibblePath::new_from_byte_array(bytes.as_slice(), 4);
        assert_eq!(nibble_path.len(), 4);
        assert_eq!(nibble_path.get_nibble(0), Nibble::from(0x01));
        assert_eq!(nibble_path.get_nibble(1), Nibble::from(0x02));
        assert_eq!(nibble_path.get_nibble(2), Nibble::from(0x03));
        assert_eq!(nibble_path.get_nibble(3), Nibble::from(0x04));

        let nibble_path = NibblePath::new_from_byte_array(bytes.as_slice(), 3);
        assert_eq!(nibble_path.len(), 3);
        assert_eq!(nibble_path.get_nibble(0), Nibble::from(0x01));
        assert_eq!(nibble_path.get_nibble(1), Nibble::from(0x02));
        assert_eq!(nibble_path.get_nibble(2), Nibble::from(0x03));
    }

    #[test]
    fn test_nibble_iterator() {
        let bytes = vec![0x12, 0x30];
        let nibble_path = NibblePath::new_odd(bytes);
        let mut iter = nibble_path.nibbles();
        assert_eq!(iter.next().unwrap(), Nibble::from(0x01));
        assert_eq!(iter.next().unwrap(), Nibble::from(0x02));
        assert_eq!(iter.next().unwrap(), Nibble::from(0x03));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_get_bit() {
        let bytes = vec![0x01, 0x02];
        let nibble_path = NibblePath::new_even(bytes);
        assert!(!nibble_path.get_bit(0));
        assert!(!nibble_path.get_bit(1));
        assert!(!nibble_path.get_bit(2));
        assert!(nibble_path.get_bit(7));
        assert!(!nibble_path.get_bit(8));
        assert!(nibble_path.get_bit(14));
    }

    #[test]
    fn test_bit_iter() {
        let bytes = vec![0xC3, 0xA0];
        let nibble_path = NibblePath::new_odd(bytes);
        let mut iter = nibble_path.bits();
        // c: 0b1100
        assert_eq!(iter.next(), Some(true));
        assert_eq!(iter.next(), Some(true));
        assert_eq!(iter.next(), Some(false));
        assert_eq!(iter.next(), Some(false));
        // 3: 0b0011
        assert_eq!(iter.next(), Some(false));
        assert_eq!(iter.next(), Some(false));
        assert_eq!(iter.next(), Some(true));
        assert_eq!(iter.next(), Some(true));
        // a: 0b1010
        assert_eq!(iter.next_back(), Some(false));
        assert_eq!(iter.next_back(), Some(true));
        assert_eq!(iter.next_back(), Some(false));
        assert_eq!(iter.next_back(), Some(true));

        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_visited_nibble_iter() {
        let bytes = vec![0x12, 0x34, 0x56];
        let nibble_path = NibblePath::new_even(bytes);
        let mut iter = nibble_path.nibbles();
        assert_eq!(iter.next().unwrap(), 0x01.into());
        assert_eq!(iter.next().unwrap(), 0x02.into());
        assert_eq!(iter.next().unwrap(), 0x03.into());
        let mut visited_nibble_iter = iter.visited_nibbles();
        assert_eq!(visited_nibble_iter.next().unwrap(), 0x01.into());
        assert_eq!(visited_nibble_iter.next().unwrap(), 0x02.into());
        assert_eq!(visited_nibble_iter.next().unwrap(), 0x03.into());
    }

    #[test]
    fn test_skip_common_prefix() {
        {
            let nibble_path1 = NibblePath::new_even(vec![0x12, 0x34, 0x56]);
            let nibble_path2 = NibblePath::new_even(vec![0x12, 0x34, 0x56]);
            let mut iter1 = nibble_path1.nibbles();
            let mut iter2 = nibble_path2.nibbles();
            assert_eq!(skip_common_prefix(&mut iter1, &mut iter2), 6);
            assert!(iter1.is_finished());
            assert!(iter2.is_finished());
        }
        {
            let nibble_path1 = NibblePath::new_even(vec![0x12, 0x35]);
            let nibble_path2 = NibblePath::new_even(vec![0x12, 0x34, 0x56]);
            let mut iter1 = nibble_path1.nibbles();
            let mut iter2 = nibble_path2.nibbles();
            assert_eq!(skip_common_prefix(&mut iter1, &mut iter2), 3);
            assert_eq!(
                iter1.visited_nibbles().get_nibble_path(),
                iter2.visited_nibbles().get_nibble_path()
            );
            assert_eq!(
                iter1.remaining_nibbles().get_nibble_path(),
                NibblePath::new_odd(vec![0x50])
            );
            assert_eq!(
                iter2.remaining_nibbles().get_nibble_path(),
                NibblePath::new_odd(vec![0x45, 0x60])
            );
        }
        {
            let nibble_path1 = NibblePath::new_even(vec![0x12, 0x34, 0x56]);
            let nibble_path2 = NibblePath::new_odd(vec![0x12, 0x30]);
            let mut iter1 = nibble_path1.nibbles();
            let mut iter2 = nibble_path2.nibbles();
            assert_eq!(skip_common_prefix(&mut iter1, &mut iter2), 3);
            assert_eq!(
                iter1.visited_nibbles().get_nibble_path(),
                iter2.visited_nibbles().get_nibble_path()
            );
            assert_eq!(
                iter1.remaining_nibbles().get_nibble_path(),
                NibblePath::new_odd(vec![0x45, 0x60])
            );
            assert!(iter2.is_finished());
        }
    }

    prop_compose! {
        fn arb_nibble_path_and_current()(nibble_path in any::<NibblePath>())
            (current in 0..=nibble_path.len(),
             nibble_path in Just(nibble_path)) -> (usize, NibblePath) {
            (current, nibble_path)
        }
    }

    proptest! {
        #[test]
        fn test_push(
            nibble_path in arb_internal_nibble_path(),
            nibble in any::<Nibble>()
        ) {
            let mut new_nibble_path = nibble_path.clone();
            new_nibble_path.push(nibble);
            let mut nibbles: Vec<Nibble> = nibble_path.nibbles().collect();
            nibbles.push(nibble);
            let nibble_path2 = nibbles.into_iter().collect();
            prop_assert_eq!(new_nibble_path, nibble_path2);
        }

        #[test]
        fn test_pop(mut nibble_path in any::<NibblePath>()) {
            let mut nibbles: Vec<Nibble> = nibble_path.nibbles().collect();
            let nibble_from_nibbles = nibbles.pop();
            let nibble_from_nibble_path = nibble_path.pop();
            let nibble_path2 = nibbles.into_iter().collect();
            prop_assert_eq!(nibble_path, nibble_path2);
            prop_assert_eq!(nibble_from_nibbles, nibble_from_nibble_path);
        }

        #[test]
        fn test_last(mut nibble_path in any::<NibblePath>()) {
            let nibble1 = nibble_path.last();
            let nibble2 = nibble_path.pop();
            prop_assert_eq!(nibble1, nibble2);
        }

        #[test]
        fn test_nibble_iter_roundtrip(nibble_path in any::<NibblePath>()) {
            let nibbles = nibble_path.nibbles();
            let nibble_path2 = nibbles.collect();
            prop_assert_eq!(nibble_path, nibble_path2);
        }

        #[test]
        fn test_visited_and_remaining_nibbles((current, nibble_path) in arb_nibble_path_and_current()) {
            let mut nibble_iter = nibble_path.nibbles();
            let mut visited_nibbles = vec![];
            for _ in 0..current {
                visited_nibbles.push(nibble_iter.next().unwrap());
            }
            let visited_nibble_path = nibble_iter.visited_nibbles().get_nibble_path();
            let remaining_nibble_path = nibble_iter.remaining_nibbles().get_nibble_path();
            let visited_iter = visited_nibble_path.nibbles();
            let remaining_iter = remaining_nibble_path.nibbles();
            prop_assert_eq!(visited_nibbles, visited_iter.collect::<Vec<Nibble>>());
            prop_assert_eq!(nibble_iter.collect::<Vec<Nibble>>(), remaining_iter.collect::<Vec<_>>());
       }

       #[test]
        fn test_nibble_iter_to_bit_iter((current, nibble_path) in arb_nibble_path_and_current()) {
            let mut nibble_iter = nibble_path.nibbles();
            (0..current)
                .for_each(|_| {
                    nibble_iter.next().unwrap();
                }
            );
            let remaining_nibble_path = nibble_iter.remaining_nibbles().get_nibble_path();
            let remaining_bit_iter = remaining_nibble_path.bits();
            let bit_iter = nibble_iter.bits();
            prop_assert_eq!(remaining_bit_iter.collect::<Vec<bool>>(), bit_iter.collect::<Vec<_>>());
        }
    }
}
