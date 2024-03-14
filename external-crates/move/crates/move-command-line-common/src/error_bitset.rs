// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

const TAG_MASK: u64 = 0x8000_0000_0000_0000;
const ERROR_CODE_MASK: u64 = 0x7fff_0000_0000_0000;
const LINE_NUMBER_MASK: u64 = 0x0000_ffff_0000_0000;
const IDENTIFIER_INDEX_MASK: u64 = 0x0000_0000_ffff_0000;
const CONSTANT_INDEX_MASK: u64 = 0x0000_0000_0000_ffff;

pub struct ErrorBitset {
    // |<tagbit>|<error code>|<line number>|<identifier index>|<constant index>|
    //   1-bit      15-bits      16-bits        16-bits          16-bits
    pub bits: u64,
}

impl ErrorBitset {
    pub fn new(
        line_number: u16,
        error_code: u16,
        identifier_index: u16,
        constant_index: u16,
    ) -> Self {
        let mut bits = 0u64;
        bits |= 1u64 << 63;
        // OK to shift over by 48 because we know the error code is only 15 bits and therefore will
        // not affect the tag bit.
        bits |= (error_code as u64) << 48;
        bits |= (line_number as u64) << 32;
        bits |= (identifier_index as u64) << 16;
        bits |= constant_index as u64;
        Self { bits }
    }

    pub fn from_u64(bits: u64) -> Option<Self> {
        if Self::is_tagged_error(bits) {
            Some(Self { bits })
        } else {
            None
        }
    }

    pub fn is_tagged_error(bits: u64) -> bool {
        (bits & TAG_MASK) >> 63 == 1
    }

    pub fn line_number(&self) -> u16 {
        ((self.bits & LINE_NUMBER_MASK) >> 32) as u16
    }

    pub fn error_code(&self) -> u16 {
        ((self.bits & ERROR_CODE_MASK) >> 48) as u16
    }

    pub fn identifier_index(&self) -> Option<u16> {
        let idx = ((self.bits & IDENTIFIER_INDEX_MASK) >> 16) as u16;
        if idx == u16::MAX {
            None
        } else {
            Some(idx)
        }
    }

    pub fn constant_index(&self) -> Option<u16> {
        // NB: purposeful truncation
        let idx = (self.bits & CONSTANT_INDEX_MASK) as u16;
        if idx == u16::MAX {
            None
        } else {
            Some(idx)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ErrorBitset;
    use proptest::prelude::*;
    use proptest::proptest;

    proptest! {
        #[test]
        fn test_error_bitset(line_number in 0..u16::MAX, error_code in 0..32768u16, identifier_index in 0..u16::MAX, constant_index in 0..u16::MAX) {
            let error_bitset = ErrorBitset::new(line_number, error_code, identifier_index, constant_index);
            prop_assert_eq!(error_bitset.line_number(), line_number);
            prop_assert_eq!(error_bitset.error_code(), error_code);
            prop_assert_eq!(error_bitset.identifier_index(), Some(identifier_index));
            prop_assert_eq!(error_bitset.constant_index(), Some(constant_index));
        }
    }
}
