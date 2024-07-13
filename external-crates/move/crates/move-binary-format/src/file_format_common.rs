// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Constants for the binary format.
//!
//! Definition for the constants of the binary format, used by the serializer and the deserializer.
//! This module also offers helpers for the serialization and deserialization of certain
//! integer indexes.
//!
//! We use LEB128 for integer compression. LEB128 is a representation from the DWARF3 spec,
//! http://dwarfstd.org/Dwarf3Std.php or https://en.wikipedia.org/wiki/LEB128.
//! It's used to compress mostly indexes into the main binary tables.
use crate::file_format::Bytecode;
use anyhow::{bail, Result};
use move_core_types as MVT;
use std::{
    io::{Cursor, Read},
    mem::size_of,
};

// Static assertions about the encoding of the flavor into the version of the binary format.
const _: () = {
    let x = BinaryFlavor::shift_and_flavor(0u32);
    // Make sure that the flavoring is added in the correct position in the u32.
    // It should always be `0x05XX_XXXX` where `XX_XXXX` is the version digits.
    assert!(x == 0x0500_0000u32);
    // Make sure that the flavoring is extracted correctly.
    assert!(BinaryFlavor::mask_and_shift_to_unflavor(x) == BinaryFlavor::SUI_FLAVOR);
};

/// Encoding of a the flavor into the version of the binary format for versions >= 7.
pub struct BinaryFlavor;
impl BinaryFlavor {
    pub const FLAVOR_MASK: u32 = 0xFF00_0000;
    pub const VERSION_MASK: u32 = 0x00FF_FFFF;
    // The Sui flavor is 0x05
    pub const SUI_FLAVOR: u8 = 0x05;
    const SHIFT_AMOUNT: u8 = 24;

    pub fn encode_version(unflavored_version: u32) -> u32 {
        if unflavored_version <= VERSION_6 {
            return unflavored_version;
        }

        debug_assert!(unflavored_version & Self::VERSION_MASK == unflavored_version);
        Self::shift_and_flavor(unflavored_version)
    }

    pub fn decode_version(flavored_version: u32) -> u32 {
        if flavored_version <= VERSION_6 {
            return flavored_version;
        }
        flavored_version & Self::VERSION_MASK
    }

    pub fn decode_flavor(flavored_version: u32) -> Option<u8> {
        if flavored_version <= VERSION_6 {
            return None;
        }
        Some(Self::mask_and_shift_to_unflavor(flavored_version))
    }

    const fn mask_and_shift_to_unflavor(flavored: u32) -> u8 {
        ((flavored & Self::FLAVOR_MASK) >> Self::SHIFT_AMOUNT) as u8
    }

    const fn shift_and_flavor(unflavored: u32) -> u32 {
        (Self::SUI_FLAVOR as u32) << Self::SHIFT_AMOUNT | unflavored
    }
}

/// Constant values for the binary format header.
///
/// The binary header is magic +  version info + table count.
pub enum BinaryConstants {}
impl BinaryConstants {
    /// The blob that must start a binary.
    pub const MOVE_MAGIC_SIZE: usize = 4;
    pub const MOVE_MAGIC: [u8; BinaryConstants::MOVE_MAGIC_SIZE] = [0xA1, 0x1C, 0xEB, 0x0B];
    /// The `DIEM_MAGIC` size, 4 byte for major version and 1 byte for table count.
    pub const HEADER_SIZE: usize = BinaryConstants::MOVE_MAGIC_SIZE + 5;
    /// A (Table Type, Start Offset, Byte Count) size, which is 1 byte for the type and
    /// 4 bytes for the offset/count.
    pub const TABLE_HEADER_SIZE: u8 = size_of::<u32>() as u8 * 2 + 1;
}

pub const TABLE_COUNT_MAX: u64 = 255;

pub const TABLE_OFFSET_MAX: u64 = 0xffff_ffff;
pub const TABLE_SIZE_MAX: u64 = 0xffff_ffff;
pub const TABLE_CONTENT_SIZE_MAX: u64 = 0xffff_ffff;

pub const TABLE_INDEX_MAX: u64 = 65535;
pub const SIGNATURE_INDEX_MAX: u64 = TABLE_INDEX_MAX;
pub const ADDRESS_INDEX_MAX: u64 = TABLE_INDEX_MAX;
pub const IDENTIFIER_INDEX_MAX: u64 = TABLE_INDEX_MAX;
pub const MODULE_HANDLE_INDEX_MAX: u64 = TABLE_INDEX_MAX;
pub const DATATYPE_HANDLE_INDEX_MAX: u64 = TABLE_INDEX_MAX;
pub const STRUCT_DEF_INDEX_MAX: u64 = TABLE_INDEX_MAX;
pub const ENUM_DEF_INDEX_MAX: u64 = TABLE_INDEX_MAX;
pub const FUNCTION_HANDLE_INDEX_MAX: u64 = TABLE_INDEX_MAX;
pub const FUNCTION_INST_INDEX_MAX: u64 = TABLE_INDEX_MAX;
pub const FIELD_HANDLE_INDEX_MAX: u64 = TABLE_INDEX_MAX;
pub const FIELD_INST_INDEX_MAX: u64 = TABLE_INDEX_MAX;
pub const STRUCT_DEF_INST_INDEX_MAX: u64 = TABLE_INDEX_MAX;
pub const ENUM_DEF_INST_INDEX_MAX: u64 = TABLE_INDEX_MAX;
pub const CONSTANT_INDEX_MAX: u64 = TABLE_INDEX_MAX;

pub const BYTECODE_COUNT_MAX: u64 = 65535;
pub const BYTECODE_INDEX_MAX: u64 = 65535;

pub const LOCAL_INDEX_MAX: u64 = 255;

pub const IDENTIFIER_SIZE_MAX: u64 = 65535;

pub const CONSTANT_SIZE_MAX: u64 = 65535;

pub const METADATA_KEY_SIZE_MAX: u64 = 1023;
pub const METADATA_VALUE_SIZE_MAX: u64 = 65535;

pub const SIGNATURE_SIZE_MAX: u64 = 255;

pub const ACQUIRES_COUNT_MAX: u64 = 255;

pub const FIELD_COUNT_MAX: u64 = 255;
pub const FIELD_OFFSET_MAX: u64 = 255;

#[allow(clippy::assertions_on_constants)]
pub const VARIANT_COUNT_MAX: u64 = {
    // These assertions are performed at compile time to ensure that the encoding of enum variants
    // does not exceed 127.
    assert!(
        MVT::VARIANT_COUNT_MAX == 127,
        "MVT::VARIANT_COUNT_MAX changed -- don't update this to more than 127 \
         without adding uleb encoding for enum variants"
    );
    MVT::VARIANT_COUNT_MAX
};

#[allow(clippy::assertions_on_constants)]
pub const JUMP_TABLE_INDEX_MAX: u64 = {
    assert!(
        MVT::VARIANT_COUNT_MAX == 127,
        "MVT::VARIANT_COUNT_MAX changed -- don't update this to more than 127 \
         without adding uleb encoding for enum variants"
    );
    MVT::VARIANT_COUNT_MAX
};

pub const VARIANT_INSTANTIATION_HANDLE_INDEX_MAX: u64 = 1024;
pub const VARIANT_HANDLE_INDEX_MAX: u64 = 1024;

pub const TYPE_PARAMETER_COUNT_MAX: u64 = 255;
pub const TYPE_PARAMETER_INDEX_MAX: u64 = 65536;

pub const SIGNATURE_TOKEN_DEPTH_MAX: usize = 256;

/// Constants for table types in the binary.
///
/// The binary contains a subset of those tables. A table specification is a tuple (table type,
/// start offset, byte count) for a given table.
#[rustfmt::skip]
#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TableType {
    MODULE_HANDLES        = 0x1,
    DATATYPE_HANDLES      = 0x2,
    FUNCTION_HANDLES      = 0x3,
    FUNCTION_INST         = 0x4,
    SIGNATURES            = 0x5,
    CONSTANT_POOL         = 0x6,
    IDENTIFIERS           = 0x7,
    ADDRESS_IDENTIFIERS   = 0x8,
    STRUCT_DEFS           = 0xA,
    STRUCT_DEF_INST       = 0xB,
    FUNCTION_DEFS         = 0xC,
    FIELD_HANDLE          = 0xD,
    FIELD_INST            = 0xE,
    FRIEND_DECLS          = 0xF,
    METADATA              = 0x10,
    ENUM_DEFS             = 0x11,
    ENUM_DEF_INST         = 0x12,
    VARIANT_HANDLES       = 0x13,
    VARIANT_INST_HANDLES  = 0x14,
}

/// Constants for signature blob values.
#[rustfmt::skip]
#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum SerializedType {
    BOOL                    = 0x1,
    U8                      = 0x2,
    U64                     = 0x3,
    U128                    = 0x4,
    ADDRESS                 = 0x5,
    REFERENCE               = 0x6,
    MUTABLE_REFERENCE       = 0x7,
    STRUCT                  = 0x8,
    TYPE_PARAMETER          = 0x9,
    VECTOR                  = 0xA,
    DATATYPE_INST           = 0xB,
    SIGNER                  = 0xC,
    U16                     = 0xD,
    U32                     = 0xE,
    U256                    = 0xF,
}

#[rustfmt::skip]
#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum SerializedNativeStructFlag {
    NATIVE   = 0x1,
    DECLARED = 0x2,
}

#[rustfmt::skip]
#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum SerializedEnumFlag {
    // 0x1 is reserved for NATIVE if we ever decide to add it
    DECLARED = 0x2, 
}

#[rustfmt::skip]
#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum SerializedJumpTableFlag {
    FULL = 0x1,
}

/// List of opcodes constants.
#[rustfmt::skip]
#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum Opcodes {
    POP                            = 0x01,
    RET                            = 0x02,
    BR_TRUE                        = 0x03,
    BR_FALSE                       = 0x04,
    BRANCH                         = 0x05,
    LD_U64                         = 0x06,
    LD_CONST                       = 0x07,
    LD_TRUE                        = 0x08,
    LD_FALSE                       = 0x09,
    COPY_LOC                       = 0x0A,
    MOVE_LOC                       = 0x0B,
    ST_LOC                         = 0x0C,
    MUT_BORROW_LOC                 = 0x0D,
    IMM_BORROW_LOC                 = 0x0E,
    MUT_BORROW_FIELD               = 0x0F,
    IMM_BORROW_FIELD               = 0x10,
    CALL                           = 0x11,
    PACK                           = 0x12,
    UNPACK                         = 0x13,
    READ_REF                       = 0x14,
    WRITE_REF                      = 0x15,
    ADD                            = 0x16,
    SUB                            = 0x17,
    MUL                            = 0x18,
    MOD                            = 0x19,
    DIV                            = 0x1A,
    BIT_OR                         = 0x1B,
    BIT_AND                        = 0x1C,
    XOR                            = 0x1D,
    OR                             = 0x1E,
    AND                            = 0x1F,
    NOT                            = 0x20,
    EQ                             = 0x21,
    NEQ                            = 0x22,
    LT                             = 0x23,
    GT                             = 0x24,
    LE                             = 0x25,
    GE                             = 0x26,
    ABORT                          = 0x27,
    NOP                            = 0x28,
    // gap for deprecated bytecodes, see bottom of enum
    FREEZE_REF                     = 0x2E,
    SHL                            = 0x2F,
    SHR                            = 0x30,
    LD_U8                          = 0x31,
    LD_U128                        = 0x32,
    CAST_U8                        = 0x33,
    CAST_U64                       = 0x34,
    CAST_U128                      = 0x35,
    MUT_BORROW_FIELD_GENERIC       = 0x36,
    IMM_BORROW_FIELD_GENERIC       = 0x37,
    CALL_GENERIC                   = 0x38,
    PACK_GENERIC                   = 0x39,
    UNPACK_GENERIC                 = 0x3A,
    VEC_PACK                       = 0x40,
    VEC_LEN                        = 0x41,
    VEC_IMM_BORROW                 = 0x42,
    VEC_MUT_BORROW                 = 0x43,
    VEC_PUSH_BACK                  = 0x44,
    VEC_POP_BACK                   = 0x45,
    VEC_UNPACK                     = 0x46,
    VEC_SWAP                       = 0x47,
    LD_U16                         = 0x48,
    LD_U32                         = 0x49,
    LD_U256                        = 0x4A,
    CAST_U16                       = 0x4B,
    CAST_U32                       = 0x4C,
    CAST_U256                      = 0x4D,
    PACK_VARIANT                   = 0x4E,
    PACK_VARIANT_GENERIC           = 0x4F,
    UNPACK_VARIANT                 = 0x50,
    UNPACK_VARIANT_IMM_REF         = 0x51,
    UNPACK_VARIANT_MUT_REF         = 0x52,
    UNPACK_VARIANT_GENERIC         = 0x53,
    UNPACK_VARIANT_GENERIC_IMM_REF = 0x54,
    UNPACK_VARIANT_GENERIC_MUT_REF = 0x55,
    VARIANT_SWITCH                 = 0x56,

    // ******** DEPRECATED BYTECODES ********
    // global storage opcodes are unused and deprecated
    EXISTS_DEPRECATED                       = 0x29,
    MUT_BORROW_GLOBAL_DEPRECATED            = 0x2A,
    IMM_BORROW_GLOBAL_DEPRECATED            = 0x2B,
    MOVE_FROM_DEPRECATED                    = 0x2C,
    MOVE_TO_DEPRECATED                      = 0x2D,
    EXISTS_GENERIC_DEPRECATED               = 0x3B,
    MUT_BORROW_GLOBAL_GENERIC_DEPRECATED    = 0x3C,
    IMM_BORROW_GLOBAL_GENERIC_DEPRECATED    = 0x3D,
    MOVE_FROM_GENERIC_DEPRECATED            = 0x3E,
    MOVE_TO_GENERIC_DEPRECATED              = 0x3F,
}

/// Upper limit on the binary size
pub const BINARY_SIZE_LIMIT: usize = usize::max_value();

/// A wrapper for the binary vector
#[derive(Default, Debug)]
pub(crate) struct BinaryData {
    _binary: Vec<u8>,
}

/// The wrapper mirrors Vector operations but provides additional checks against overflow
impl BinaryData {
    pub fn new() -> Self {
        BinaryData {
            _binary: Vec::new(),
        }
    }

    pub fn as_inner(&self) -> &[u8] {
        &self._binary
    }

    pub fn into_inner(self) -> Vec<u8> {
        self._binary
    }

    pub fn push(&mut self, item: u8) -> Result<()> {
        if self.len().checked_add(1).is_some() {
            self._binary.push(item);
        } else {
            bail!(
                "binary size ({}) + 1 is greater than limit ({})",
                self.len(),
                BINARY_SIZE_LIMIT,
            );
        }
        Ok(())
    }

    pub fn extend(&mut self, vec: &[u8]) -> Result<()> {
        let vec_len: usize = vec.len();
        if self.len().checked_add(vec_len).is_some() {
            self._binary.extend(vec);
        } else {
            bail!(
                "binary size ({}) + {} is greater than limit ({})",
                self.len(),
                vec.len(),
                BINARY_SIZE_LIMIT,
            );
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self._binary.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self._binary.is_empty()
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self._binary.clear();
    }
}

impl From<Vec<u8>> for BinaryData {
    fn from(vec: Vec<u8>) -> Self {
        BinaryData { _binary: vec }
    }
}

pub(crate) fn write_u64_as_uleb128(binary: &mut BinaryData, mut val: u64) -> Result<()> {
    loop {
        let cur = val & 0x7f;
        if cur != val {
            binary.push((cur | 0x80) as u8)?;
            val >>= 7;
        } else {
            binary.push(cur as u8)?;
            break;
        }
    }
    Ok(())
}

/// Write a `u16` in Little Endian format.
#[allow(dead_code)]
pub(crate) fn write_u16(binary: &mut BinaryData, value: u16) -> Result<()> {
    binary.extend(&value.to_le_bytes())
}

/// Write a `u32` in Little Endian format.
pub(crate) fn write_u32(binary: &mut BinaryData, value: u32) -> Result<()> {
    binary.extend(&value.to_le_bytes())
}

/// Write a `u64` in Little Endian format.
pub(crate) fn write_u64(binary: &mut BinaryData, value: u64) -> Result<()> {
    binary.extend(&value.to_le_bytes())
}

/// Write a `u128` in Little Endian format.
pub(crate) fn write_u128(binary: &mut BinaryData, value: u128) -> Result<()> {
    binary.extend(&value.to_le_bytes())
}

/// Write a `u256` in Little Endian format.
pub(crate) fn write_u256(
    binary: &mut BinaryData,
    value: move_core_types::u256::U256,
) -> Result<()> {
    binary.extend(&value.to_le_bytes())
}

pub fn read_u8(cursor: &mut Cursor<&[u8]>) -> Result<u8> {
    let mut buf = [0; 1];
    cursor.read_exact(&mut buf)?;
    Ok(buf[0])
}

pub fn read_u32(cursor: &mut Cursor<&[u8]>) -> Result<u32> {
    let mut buf = [0; 4];
    cursor.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

pub fn read_uleb128_as_u64(cursor: &mut Cursor<&[u8]>) -> Result<u64> {
    let mut value: u64 = 0;
    let mut shift = 0;
    while let Ok(byte) = read_u8(cursor) {
        let cur = (byte & 0x7f) as u64;
        if (cur << shift) >> shift != cur {
            bail!("invalid ULEB128 repr for usize");
        }
        value |= cur << shift;

        if (byte & 0x80) == 0 {
            if shift > 0 && cur == 0 {
                bail!("invalid ULEB128 repr for usize");
            }
            return Ok(value);
        }

        shift += 7;
        if shift > u64::BITS {
            break;
        }
    }
    bail!("invalid ULEB128 repr for usize");
}

//
// Bytecode evolution
//

/// Version 1: the initial version
pub const VERSION_1: u32 = 1;

/// Version 2: changes compared with version 1
///  + function visibility stored in separate byte before the flags byte
///  + the flags byte now contains only the is_native information (at bit 0x2)
///  + new visibility modifiers for "friend" and "script" functions
///  + friend list for modules
pub const VERSION_2: u32 = 2;

/// Version 3: changes compared with version 2
///  + phantom type parameters
pub const VERSION_3: u32 = 3;

/// Version 4: changes compared with version 3
///  + bytecode for vector operations
pub const VERSION_4: u32 = 4;

/// Version 5: changes compared with version 4
///  +/- script and public(script) verification is now adapter specific
///  + metadata
pub const VERSION_5: u32 = 5;

/// Version 6: changes compared with version 5
///  + u16, u32, u256 integers and corresponding Ld, Cast bytecodes
pub const VERSION_6: u32 = 6;

/// Version 7: changes compared with version 6
///  + enums
pub const VERSION_7: u32 = 7;

// Mark which version is the latest version
pub const VERSION_MAX: u32 = VERSION_7;

// Mark which oldest version is supported.
// TODO(#145): finish v4 compatibility; as of now, only metadata is implemented
pub const VERSION_MIN: u32 = VERSION_5;

/// The encoding of the instruction is the serialized form of it, but disregarding the
/// serialization of the instruction's argument(s).
pub fn instruction_key(instruction: &Bytecode) -> u8 {
    use Bytecode::*;
    let opcode = match instruction {
        Pop => Opcodes::POP,
        Ret => Opcodes::RET,
        BrTrue(_) => Opcodes::BR_TRUE,
        BrFalse(_) => Opcodes::BR_FALSE,
        Branch(_) => Opcodes::BRANCH,
        LdU8(_) => Opcodes::LD_U8,
        LdU64(_) => Opcodes::LD_U64,
        LdU128(_) => Opcodes::LD_U128,
        CastU8 => Opcodes::CAST_U8,
        CastU64 => Opcodes::CAST_U64,
        CastU128 => Opcodes::CAST_U128,
        LdConst(_) => Opcodes::LD_CONST,
        LdTrue => Opcodes::LD_TRUE,
        LdFalse => Opcodes::LD_FALSE,
        CopyLoc(_) => Opcodes::COPY_LOC,
        MoveLoc(_) => Opcodes::MOVE_LOC,
        StLoc(_) => Opcodes::ST_LOC,
        Call(_) => Opcodes::CALL,
        CallGeneric(_) => Opcodes::CALL_GENERIC,
        Pack(_) => Opcodes::PACK,
        PackGeneric(_) => Opcodes::PACK_GENERIC,
        Unpack(_) => Opcodes::UNPACK,
        UnpackGeneric(_) => Opcodes::UNPACK_GENERIC,
        ReadRef => Opcodes::READ_REF,
        WriteRef => Opcodes::WRITE_REF,
        FreezeRef => Opcodes::FREEZE_REF,
        MutBorrowLoc(_) => Opcodes::MUT_BORROW_LOC,
        ImmBorrowLoc(_) => Opcodes::IMM_BORROW_LOC,
        MutBorrowField(_) => Opcodes::MUT_BORROW_FIELD,
        MutBorrowFieldGeneric(_) => Opcodes::MUT_BORROW_FIELD_GENERIC,
        ImmBorrowField(_) => Opcodes::IMM_BORROW_FIELD,
        ImmBorrowFieldGeneric(_) => Opcodes::IMM_BORROW_FIELD_GENERIC,
        Add => Opcodes::ADD,
        Sub => Opcodes::SUB,
        Mul => Opcodes::MUL,
        Mod => Opcodes::MOD,
        Div => Opcodes::DIV,
        BitOr => Opcodes::BIT_OR,
        BitAnd => Opcodes::BIT_AND,
        Xor => Opcodes::XOR,
        Shl => Opcodes::SHL,
        Shr => Opcodes::SHR,
        Or => Opcodes::OR,
        And => Opcodes::AND,
        Not => Opcodes::NOT,
        Eq => Opcodes::EQ,
        Neq => Opcodes::NEQ,
        Lt => Opcodes::LT,
        Gt => Opcodes::GT,
        Le => Opcodes::LE,
        Ge => Opcodes::GE,
        Abort => Opcodes::ABORT,
        Nop => Opcodes::NOP,
        VecPack(..) => Opcodes::VEC_PACK,
        VecLen(_) => Opcodes::VEC_LEN,
        VecImmBorrow(_) => Opcodes::VEC_IMM_BORROW,
        VecMutBorrow(_) => Opcodes::VEC_MUT_BORROW,
        VecPushBack(_) => Opcodes::VEC_PUSH_BACK,
        VecPopBack(_) => Opcodes::VEC_POP_BACK,
        VecUnpack(..) => Opcodes::VEC_UNPACK,
        VecSwap(_) => Opcodes::VEC_SWAP,
        LdU16(_) => Opcodes::LD_U16,
        LdU32(_) => Opcodes::LD_U32,
        LdU256(_) => Opcodes::LD_U256,
        CastU16 => Opcodes::CAST_U16,
        CastU32 => Opcodes::CAST_U32,
        CastU256 => Opcodes::CAST_U256,
        PackVariant(_) => Opcodes::PACK_VARIANT,
        PackVariantGeneric(_) => Opcodes::PACK_VARIANT_GENERIC,
        UnpackVariant(_) => Opcodes::UNPACK_VARIANT,
        UnpackVariantImmRef(_) => Opcodes::UNPACK_VARIANT_IMM_REF,
        UnpackVariantMutRef(_) => Opcodes::UNPACK_VARIANT_MUT_REF,
        UnpackVariantGeneric(_) => Opcodes::UNPACK_VARIANT_GENERIC,
        UnpackVariantGenericImmRef(_) => Opcodes::UNPACK_VARIANT_GENERIC_IMM_REF,
        UnpackVariantGenericMutRef(_) => Opcodes::UNPACK_VARIANT_GENERIC_MUT_REF,
        VariantSwitch(_) => Opcodes::VARIANT_SWITCH,
        // ******** DEPRECATED BYTECODES ********
        ExistsDeprecated(_) => Opcodes::EXISTS_DEPRECATED,
        ExistsGenericDeprecated(_) => Opcodes::EXISTS_GENERIC_DEPRECATED,
        MoveFromDeprecated(_) => Opcodes::MOVE_FROM_DEPRECATED,
        MoveFromGenericDeprecated(_) => Opcodes::MOVE_FROM_GENERIC_DEPRECATED,
        MoveToDeprecated(_) => Opcodes::MOVE_TO_DEPRECATED,
        MoveToGenericDeprecated(_) => Opcodes::MOVE_TO_GENERIC_DEPRECATED,
        MutBorrowGlobalDeprecated(_) => Opcodes::MUT_BORROW_GLOBAL_DEPRECATED,
        MutBorrowGlobalGenericDeprecated(_) => Opcodes::MUT_BORROW_GLOBAL_GENERIC_DEPRECATED,
        ImmBorrowGlobalDeprecated(_) => Opcodes::IMM_BORROW_GLOBAL_DEPRECATED,
        ImmBorrowGlobalGenericDeprecated(_) => Opcodes::IMM_BORROW_GLOBAL_GENERIC_DEPRECATED,
    };
    opcode as u8
}
