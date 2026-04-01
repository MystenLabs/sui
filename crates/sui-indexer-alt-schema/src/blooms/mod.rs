// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::StructTag;
use sui_types::SUI_CLOCK_ADDRESS;

pub mod blocked;
pub mod bloom;
pub mod hash;

/// High-frequency identifiers excluded from bloom filters. These appear in most
/// checkpoints, so including them would cause queries to match nearly all blocks.
const BLOOM_SKIP_ADDRESSES: &[AccountAddress] = &[AccountAddress::ZERO, SUI_CLOCK_ADDRESS];

/// Single-byte prefix tags for bloom key dimensions that would otherwise collide.
/// Not every `BloomValue` variant is tagged — untagged variants are unlikely to
/// collide with other dimensions in practice (e.g. function names vs type params).
#[repr(u8)]
enum BloomTag {
    MoveCallPackage = b'P',
    MoveCallModule = b'M',
    EventEmitModule = b'E',
    EventAddress = b'A',
    AffectedObject = b'O',
    EventTypeModule = b'T',
}

impl BloomTag {
    fn prefix(self, data: &[u8]) -> Vec<u8> {
        [&[self as u8], data].concat()
    }
}

/// Bloom filter values indexed as individual components (package, module, name) rather than
/// compound keys (pkg::module::name). AND-probing N separate components requires N*k bloom
/// bits to match, giving better selectivity than a single compound probe (k bits).
///
/// Tags prevent collisions across dimensions (e.g. a Move call package address vs an event
/// type module address).
#[derive(Debug)]
pub enum BloomValue {
    /// Transaction sender or affected address (recipients, gas owner, etc.).
    SenderOrRecipient(AccountAddress),
    /// Object ID mutated, created, or deleted by a transaction.
    AffectedObject(AccountAddress),
    /// Move call package address.
    MoveCallPackage(AccountAddress),
    /// Move call module name.
    MoveCallModule(String),
    /// Event emitting or type package address.
    EventAddress(AccountAddress),
    /// Event emitting module name (the module whose function emitted the event).
    EventEmitModule(String),
    /// Event type module name (from the event's StructTag, not the emitting function).
    EventTypeModule(String),
    /// Function or type name (without package or module).
    Name(String),
    /// Event type parameter canonical string (e.g. "bool", "u64", "0x2::sui::SUI").
    TypeParam(String),
}

impl BloomValue {
    pub fn to_bytes(self) -> Vec<u8> {
        match self {
            Self::EventAddress(addr) => BloomTag::EventAddress.prefix(addr.as_ref()),
            Self::SenderOrRecipient(addr) => addr.to_vec(),
            Self::AffectedObject(addr) => BloomTag::AffectedObject.prefix(addr.as_ref()),
            Self::MoveCallPackage(pkg) => BloomTag::MoveCallPackage.prefix(pkg.as_ref()),
            Self::MoveCallModule(module) => BloomTag::MoveCallModule.prefix(module.as_bytes()),
            Self::EventEmitModule(module) => BloomTag::EventEmitModule.prefix(module.as_bytes()),
            Self::EventTypeModule(module) => BloomTag::EventTypeModule.prefix(module.as_bytes()),
            Self::Name(name) => name.into_bytes(),
            Self::TypeParam(s) => s.into_bytes(),
        }
    }

    /// Returns true if this value should be excluded from bloom filter operations
    /// because it matches a high-frequency address.
    pub fn exclude(&self) -> bool {
        let addr = match self {
            Self::EventAddress(addr)
            | Self::SenderOrRecipient(addr)
            | Self::AffectedObject(addr)
            | Self::MoveCallPackage(addr) => addr,
            Self::MoveCallModule(_)
            | Self::EventEmitModule(_)
            | Self::EventTypeModule(_)
            | Self::Name(_)
            | Self::TypeParam(_) => return false,
        };
        BLOOM_SKIP_ADDRESSES.contains(addr)
    }

    /// Extract bloom values from an event's StructTag for insertion into the
    /// checkpoint bloom filter.
    ///
    /// The top-level struct gets three keys at increasing specificity:
    ///   - `EventAddress(pkg)`        — matches queries filtering by package only
    ///   - `EventTypeModule(mod)`       — matches queries filtering by module
    ///   - `Name(name)`             — matches queries filtering by type name
    ///
    /// Each type parameter is inserted as its canonical string representation
    /// (e.g. "bool", "u64", "0x2::sui::SUI"). This handles all TypeTag variants
    /// including primitives and nested generics.
    pub fn from_event_struct_tag(tag: &StructTag) -> Vec<BloomValue> {
        let mut values = vec![
            BloomValue::EventAddress(tag.address),
            BloomValue::EventTypeModule(tag.module.to_string()),
            BloomValue::Name(tag.name.to_string()),
        ];
        for tp in &tag.type_params {
            values.push(BloomValue::TypeParam(tp.to_canonical_string(false)));
        }
        values
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_exclude() {
        let zero = AccountAddress::ZERO;
        let clock = SUI_CLOCK_ADDRESS;
        let normal = AccountAddress::from_hex_literal("0x42").unwrap();

        assert!(BloomValue::EventAddress(zero).exclude());
        assert!(BloomValue::EventAddress(clock).exclude());
        assert!(!BloomValue::EventAddress(normal).exclude());

        assert!(BloomValue::SenderOrRecipient(zero).exclude());
        assert!(BloomValue::AffectedObject(zero).exclude());
        assert!(!BloomValue::SenderOrRecipient(normal).exclude());
        assert!(!BloomValue::AffectedObject(normal).exclude());

        assert!(BloomValue::MoveCallPackage(zero).exclude());
        assert!(!BloomValue::MoveCallModule("mod".into()).exclude());
        assert!(!BloomValue::EventEmitModule("mod".into()).exclude());
        assert!(!BloomValue::EventTypeModule("mod".into()).exclude());
        assert!(!BloomValue::Name("name".into()).exclude());
        assert!(!BloomValue::MoveCallPackage(normal).exclude());
        assert!(!BloomValue::TypeParam("bool".into()).exclude());
        assert!(!BloomValue::TypeParam("u64".into()).exclude());
    }

    #[test]
    fn test_all_tagged_variants_differ() {
        let addr = AccountAddress::from_hex_literal("0x2").unwrap();
        let variants = [
            BloomValue::EventAddress(addr).to_bytes(),
            BloomValue::SenderOrRecipient(addr).to_bytes(),
            BloomValue::AffectedObject(addr).to_bytes(),
            BloomValue::MoveCallPackage(addr).to_bytes(),
            BloomValue::MoveCallModule("m".into()).to_bytes(),
            BloomValue::EventEmitModule("m".into()).to_bytes(),
            BloomValue::EventTypeModule("m".into()).to_bytes(),
            BloomValue::Name("n".into()).to_bytes(),
            BloomValue::TypeParam("bool".into()).to_bytes(),
        ];
        for i in 0..variants.len() {
            for j in (i + 1)..variants.len() {
                assert_ne!(variants[i], variants[j], "variants {i} and {j} collide");
            }
        }
    }
}
