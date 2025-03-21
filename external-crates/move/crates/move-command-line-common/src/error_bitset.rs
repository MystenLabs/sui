// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use packed_struct::prelude::*;

pub const BITSET_U16_UNAVAILABLE: u16 = u16::MAX;
pub const BITSET_U8_UNAVAILABLE: u8 = u8::MAX;

const VERSION_0: u8 = 0b1000;
const VERSION_1: u8 = 0b1100;
const VERSIONS: [u8; 2] = [VERSION_0, VERSION_1];

#[derive(PackedStruct, Debug, Clone, Copy, PartialEq, Eq)]
#[packed_struct(bit_numbering = "msb0", endian = "msb", size_bytes = "8")]
pub struct ErrorBitset {
    // Bit layout (from MSB to LSB):
    // | tag (1-bit) | reserved (7-bits) | code (8-bits) | line_number (16-bits) | identifier_index (16-bits) | constant_index (16-bits) |
    #[packed_field(bits = "0..=3")]
    version: Integer<u8, packed_bits::Bits<4>>,
    #[packed_field(bits = "4..=7")]
    #[allow(dead_code)]
    reserved: Integer<u8, packed_bits::Bits<4>>,
    error_code: u8,
    line_number: u16,
    identifier_index: u16,
    constant_index: u16,
}

pub struct ErrorBitsetBuilder {
    line_number: u16,
    error_code: Option<u8>,
    identifier_index: Option<u16>,
    constant_index: Option<u16>,
}

impl ErrorBitsetBuilder {
    pub fn new(line_number: u16) -> Self {
        Self {
            line_number,
            error_code: None,
            identifier_index: None,
            constant_index: None,
        }
    }

    pub fn with_error_code(&mut self, error_code: u8) {
        self.error_code = Some(error_code);
    }

    pub fn with_identifier_index(&mut self, identifier_index: u16) {
        self.identifier_index = Some(identifier_index);
    }

    pub fn with_constant_index(&mut self, constant_index: u16) {
        self.constant_index = Some(constant_index);
    }

    pub fn build(&self) -> ErrorBitset {
        ErrorBitset {
            version: VERSION_1.into(),
            reserved: 0.into(),
            error_code: self.error_code.unwrap_or(BITSET_U8_UNAVAILABLE),
            line_number: self.line_number,
            identifier_index: self.identifier_index.unwrap_or(BITSET_U16_UNAVAILABLE),
            constant_index: self.constant_index.unwrap_or(BITSET_U16_UNAVAILABLE),
        }
    }
}

impl ErrorBitset {
    /// For testing (below)
    #[cfg(test)]
    pub(crate) fn new(
        version: u8,
        error_code: u8,
        line_number: u16,
        identifier_index: u16,
        constant_index: u16,
    ) -> Self {
        ErrorBitset {
            version: version.into(),
            reserved: 0.into(),
            error_code,
            line_number,
            identifier_index,
            constant_index,
        }
    }

    pub fn bits(&self) -> u64 {
        // Pack the ErrorBitset into a Vec<u8> (should be exactly 8 bytes)
        let packed = self.pack().expect("Failed to pack ErrorBitset");
        // Convert the Vec<u8> to an array of 8 bytes
        let bytes: [u8; 8] = packed
            .as_slice()
            .try_into()
            .expect("Packed data is not 8 bytes long");
        // Convert the 8-byte array to a u64 assuming big-endian byte order
        u64::from_be_bytes(bytes)
    }

    pub fn from_u64(bits: u64) -> Option<Self> {
        let bytes = bits.to_be_bytes();
        // Unpack the 8-byte slice into an ErrorBitset instance.
        let error_bitset = ErrorBitset::unpack_from_slice(&bytes).ok()?;
        if VERSIONS.contains(&error_bitset.version) {
            Some(error_bitset)
        } else {
            None
        }
    }

    const fn u8_sentinel(v: u8) -> Option<u8> {
        if v == BITSET_U8_UNAVAILABLE {
            None
        } else {
            Some(v)
        }
    }

    const fn u16_sentinel(v: u16) -> Option<u16> {
        if v == BITSET_U16_UNAVAILABLE {
            None
        } else {
            Some(v)
        }
    }

    pub fn error_code(&self) -> Option<u8> {
        if u8::from(self.version) == VERSION_0 {
            return None;
        }
        Self::u8_sentinel(self.error_code)
    }

    pub const fn line_number(&self) -> Option<u16> {
        Self::u16_sentinel(self.line_number)
    }

    pub const fn identifier_index(&self) -> Option<u16> {
        Self::u16_sentinel(self.identifier_index)
    }

    pub const fn constant_index(&self) -> Option<u16> {
        Self::u16_sentinel(self.constant_index)
    }
}

#[cfg(test)]
mod tests {
    use crate::error_bitset::VERSION_1;

    use super::{ErrorBitset, ErrorBitsetBuilder};
    use proptest::prelude::*;
    use proptest::proptest;

    proptest! {
        #[test]
        fn test_error_bitset(error_code in 0..u8::MAX, line_number in 0..u16::MAX, identifier_index in 0..u16::MAX, constant_index in 0..u16::MAX) {
            let error_bitset = ErrorBitset::new(VERSION_1, error_code, line_number, identifier_index, constant_index);
            prop_assert_eq!(error_bitset.line_number(), Some(line_number));
            prop_assert_eq!(error_bitset.identifier_index(), Some(identifier_index));
            prop_assert_eq!(error_bitset.constant_index(), Some(constant_index));
        }
    }

    proptest! {
        #[test]
        fn test_error_bitset_builder(error_code in 0..u8::MAX, line_number in 0..u16::MAX, identifier_index in 0..u16::MAX, constant_index in 0..u16::MAX) {
            let error_bitset = ErrorBitset::new(VERSION_1, error_code, line_number, identifier_index, constant_index);
            let mut error_bitset_builder = ErrorBitsetBuilder::new(line_number);
            error_bitset_builder.with_error_code(error_code);
            error_bitset_builder.with_identifier_index(identifier_index);
            error_bitset_builder.with_constant_index(constant_index);
            let error_bitset_built = error_bitset_builder.build();
            prop_assert_eq!(error_bitset.line_number(), Some(line_number));
            prop_assert_eq!(error_bitset.identifier_index(), Some(identifier_index));
            prop_assert_eq!(error_bitset.constant_index(), Some(constant_index));

            prop_assert_eq!(error_bitset_built.line_number(), Some(line_number));
            prop_assert_eq!(error_bitset_built.identifier_index(), Some(identifier_index));
            prop_assert_eq!(error_bitset_built.constant_index(), Some(constant_index));

            prop_assert_eq!(error_bitset.bits(), error_bitset_built.bits());
        }
    }
}
