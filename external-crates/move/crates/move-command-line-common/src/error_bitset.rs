// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

const BITSET_VALUE_UNAVAILABLE: u16 = u16::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorBitsetField {
    Tag,
    #[allow(dead_code)]
    Reserved,
    LineNumber,
    Identifier,
    Constant,
}

impl ErrorBitsetField {
    const TAG_MASK: u64 = 0x8000_0000_0000_0000;
    const RESERVED_AREA_MASK: u64 = 0x7fff_0000_0000_0000;
    const LINE_NUMBER_MASK: u64 = 0x0000_ffff_0000_0000;
    const IDENTIFIER_INDEX_MASK: u64 = 0x0000_0000_ffff_0000;
    const CONSTANT_INDEX_MASK: u64 = 0x0000_0000_0000_ffff;

    const TAG_SHIFT: u64 = 63;
    const RESERVED_AREA_SHIFT: u64 = 48;
    const LINE_NUMBER_SHIFT: u64 = 32;
    const IDENTIFIER_INDEX_SHIFT: u64 = 16;
    const CONSTANT_INDEX_SHIFT: u64 = 0;

    const fn mask(&self) -> u64 {
        match self {
            Self::Tag => Self::TAG_MASK,
            Self::Reserved => Self::RESERVED_AREA_MASK,
            Self::LineNumber => Self::LINE_NUMBER_MASK,
            Self::Identifier => Self::IDENTIFIER_INDEX_MASK,
            Self::Constant => Self::CONSTANT_INDEX_MASK,
        }
    }

    const fn shift(&self) -> u64 {
        match self {
            Self::Tag => Self::TAG_SHIFT,
            Self::Reserved => Self::RESERVED_AREA_SHIFT,
            Self::LineNumber => Self::LINE_NUMBER_SHIFT,
            Self::Identifier => Self::IDENTIFIER_INDEX_SHIFT,
            Self::Constant => Self::CONSTANT_INDEX_SHIFT,
        }
    }

    const fn get_bits(&self, bits: u64) -> u16 {
        ((bits & self.mask()) >> self.shift()) as u16
    }
}

pub struct ErrorBitset {
    // |<tagbit>|<reserved>|<line number>|<identifier index>|<constant index>|
    //   1-bit    15-bits       16-bits        16-bits          16-bits
    pub bits: u64,
}

impl ErrorBitset {
    pub const fn new(line_number: u16, identifier_index: u16, constant_index: u16) -> Self {
        use ErrorBitsetField::*;
        let mut bits = 0u64;
        bits |= 1u64 << Tag.shift();
        bits |= (line_number as u64) << LineNumber.shift();
        bits |= (identifier_index as u64) << Identifier.shift();
        bits |= (constant_index as u64) << Constant.shift();
        Self { bits }
    }

    pub const fn from_u64(bits: u64) -> Option<Self> {
        if Self::is_tagged_error(bits) {
            Some(Self { bits })
        } else {
            None
        }
    }

    pub const fn is_tagged_error(bits: u64) -> bool {
        ErrorBitsetField::Tag.get_bits(bits) == 1
    }

    const fn sentinal(v: u16) -> Option<u16> {
        if v == BITSET_VALUE_UNAVAILABLE {
            None
        } else {
            Some(v)
        }
    }

    pub const fn line_number(&self) -> Option<u16> {
        Self::sentinal(ErrorBitsetField::LineNumber.get_bits(self.bits))
    }

    pub const fn identifier_index(&self) -> Option<u16> {
        Self::sentinal(ErrorBitsetField::Identifier.get_bits(self.bits))
    }

    pub const fn constant_index(&self) -> Option<u16> {
        Self::sentinal(ErrorBitsetField::Constant.get_bits(self.bits))
    }
}

#[cfg(test)]
mod tests {
    use super::ErrorBitset;
    use proptest::prelude::*;
    use proptest::proptest;

    proptest! {
        #[test]
        fn test_error_bitset(line_number in 0..u16::MAX, identifier_index in 0..u16::MAX, constant_index in 0..u16::MAX) {
            let error_bitset = ErrorBitset::new(line_number, identifier_index, constant_index);
            prop_assert_eq!(error_bitset.line_number(), Some(line_number));
            prop_assert_eq!(error_bitset.identifier_index(), Some(identifier_index));
            prop_assert_eq!(error_bitset.constant_index(), Some(constant_index));
        }
    }
}
