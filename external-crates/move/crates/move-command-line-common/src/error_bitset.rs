// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

const BITSET_VALUE_UNAVAILABLE: u16 = u16::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorBitset {
    // |<tagbit>|<reserved>|<line number>|<identifier index>|<constant index>|
    //   1-bit    15-bits       16-bits        16-bits          16-bits
    pub bits: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorBitsetField {
    Tag,
    #[allow(dead_code)]
    Reserved,
    LineNumber,
    Identifier,
    Constant,
}

pub struct ErrorBitsetBuilder {
    line_number: u16,
    identifier_index: Option<u16>,
    constant_index: Option<u16>,
}

impl ErrorBitsetField {
    const TAG_MASK: u64 = 0x8000_0000_0000_0000;
    const RESERVED_AREA_MASK: u64 = 0x7fff_0000_0000_0000;
    const LINE_NUMBER_MASK: u64 = 0x0000_ffff_0000_0000;
    const IDENTIFIER_INDEX_MASK: u64 = 0x0000_0000_ffff_0000;
    const CONSTANT_INDEX_MASK: u64 = 0x0000_0000_0000_ffff;

    const TAG_SHIFT: u64 = Self::RESERVED_AREA_SHIFT + 15;
    const RESERVED_AREA_SHIFT: u64 = Self::LINE_NUMBER_SHIFT + 16;
    const LINE_NUMBER_SHIFT: u64 = Self::IDENTIFIER_INDEX_SHIFT + 16;
    const IDENTIFIER_INDEX_SHIFT: u64 = Self::CONSTANT_INDEX_SHIFT + 16;
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

impl ErrorBitsetBuilder {
    pub fn new(line_number: u16) -> Self {
        Self {
            line_number,
            identifier_index: None,
            constant_index: None,
        }
    }

    pub fn with_identifier_index(&mut self, identifier_index: u16) {
        self.identifier_index = Some(identifier_index);
    }

    pub fn with_constant_index(&mut self, constant_index: u16) {
        self.constant_index = Some(constant_index);
    }

    pub fn build(self) -> ErrorBitset {
        ErrorBitset::new(
            self.line_number,
            self.identifier_index.unwrap_or(BITSET_VALUE_UNAVAILABLE),
            self.constant_index.unwrap_or(BITSET_VALUE_UNAVAILABLE),
        )
    }
}

impl ErrorBitset {
    pub(crate) const fn new(line_number: u16, identifier_index: u16, constant_index: u16) -> Self {
        use ErrorBitsetField as E;
        let mut bits = 0u64;
        bits |= 1u64 << E::Tag.shift();
        bits |= (line_number as u64) << E::LineNumber.shift();
        bits |= (identifier_index as u64) << E::Identifier.shift();
        bits |= (constant_index as u64) << E::Constant.shift();
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

    const fn sentinel(v: u16) -> Option<u16> {
        if v == BITSET_VALUE_UNAVAILABLE {
            None
        } else {
            Some(v)
        }
    }

    pub const fn line_number(&self) -> Option<u16> {
        Self::sentinel(ErrorBitsetField::LineNumber.get_bits(self.bits))
    }

    pub const fn identifier_index(&self) -> Option<u16> {
        Self::sentinel(ErrorBitsetField::Identifier.get_bits(self.bits))
    }

    pub const fn constant_index(&self) -> Option<u16> {
        Self::sentinel(ErrorBitsetField::Constant.get_bits(self.bits))
    }
}

#[cfg(test)]
mod tests {
    use super::{ErrorBitset, ErrorBitsetBuilder};
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

    proptest! {
        #[test]
        fn test_error_bitset_builder(line_number in 0..u16::MAX, identifier_index in 0..u16::MAX, constant_index in 0..u16::MAX) {
            let error_bitset = ErrorBitset::new(line_number, identifier_index, constant_index);
            let mut error_bitset_builder = ErrorBitsetBuilder::new(line_number);
            error_bitset_builder.with_identifier_index(identifier_index);
            error_bitset_builder.with_constant_index(constant_index);
            let error_bitset_built = error_bitset_builder.build();
            prop_assert_eq!(error_bitset.line_number(), Some(line_number));
            prop_assert_eq!(error_bitset.identifier_index(), Some(identifier_index));
            prop_assert_eq!(error_bitset.constant_index(), Some(constant_index));

            prop_assert_eq!(error_bitset_built.line_number(), Some(line_number));
            prop_assert_eq!(error_bitset_built.identifier_index(), Some(identifier_index));
            prop_assert_eq!(error_bitset_built.constant_index(), Some(constant_index));

            prop_assert_eq!(error_bitset.bits, error_bitset_built.bits);
        }
    }
}
