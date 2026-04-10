// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::ExecutedTransaction;
use sui_types::object::Owner;
use sui_types::transaction::TransactionDataAPI;

/// A queryable dimension for the checkpoint inverted index.
///
/// Each variant has a unique single-byte tag used as a prefix in row keys,
/// ensuring no two dimensions can produce the same encoded bytes.
///
/// Compound dimensions (MoveCall, EmitModule, EventType) use hierarchical
/// keys: each prefix level is a valid, independently queryable key. For
/// example, MoveCall encodes `[pkg_32]`, `[pkg_32][module]`, or
/// `[pkg_32][module\x00function]` depending on the query specificity.
/// The 32-byte address/package prefix is fixed-width (no separator needed),
/// and `\x00` separates variable-length components (safe because Move
/// identifiers cannot contain null bytes).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IndexDimension {
    Sender = 0x01,
    Recipient = 0x02,
    AffectedObject = 0x03,
    /// Compound: `[package_32]` | `[package_32][module]` | `[package_32][module\x00function]`
    MoveCall = 0x04,
    /// Compound: `[package_id_32]` | `[package_id_32][module]`
    EmitModule = 0x05,
    /// Compound: `[type_address_32]` | `[..][module]` | `[..\x00name]` | `[..\x00name\x00instantiation_bcs]`
    EventType = 0x06,
}

impl IndexDimension {
    pub fn tag_byte(self) -> u8 {
        self as u8
    }
}

/// Encode a dimension value into a row key component: `[tag_byte][value_bytes]`.
pub fn encode_dimension_key(dim: IndexDimension, value: &[u8]) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + value.len());
    write_dimension_key(&mut key, dim, value);
    key
}

/// Append a dimension key into `out` using the `[tag_byte][value_bytes]` format.
pub fn write_dimension_key(out: &mut Vec<u8>, dim: IndexDimension, value: &[u8]) {
    out.clear();
    out.reserve(1 + value.len());
    out.push(dim.tag_byte());
    out.extend_from_slice(value);
}

// --- Compound key construction helpers ---
// Used by both the write side (extract_dimensions) and the read side (filter parsing).

/// Append a MoveCall compound value at the desired specificity into `out`.
pub fn write_move_call_value(
    out: &mut Vec<u8>,
    package: &[u8],
    module: Option<&str>,
    function: Option<&str>,
) {
    out.clear();
    out.reserve(32 + 32);
    out.extend_from_slice(package);
    if let Some(m) = module {
        out.extend_from_slice(m.as_bytes());
        if let Some(f) = function {
            out.push(0x00);
            out.extend_from_slice(f.as_bytes());
        }
    }
}

/// Build a MoveCall compound value at the desired specificity.
pub fn move_call_value(package: &[u8], module: Option<&str>, function: Option<&str>) -> Vec<u8> {
    let mut v = Vec::with_capacity(32 + 32);
    write_move_call_value(&mut v, package, module, function);
    v
}

/// Append an EmitModule compound value at the desired specificity into `out`.
pub fn write_emit_module_value(out: &mut Vec<u8>, package_id: &[u8], module: Option<&str>) {
    out.clear();
    out.reserve(32 + 16);
    out.extend_from_slice(package_id);
    if let Some(m) = module {
        out.extend_from_slice(m.as_bytes());
    }
}

/// Build an EmitModule compound value at the desired specificity.
pub fn emit_module_value(package_id: &[u8], module: Option<&str>) -> Vec<u8> {
    let mut v = Vec::with_capacity(32 + 16);
    write_emit_module_value(&mut v, package_id, module);
    v
}

/// Append an EventType compound value at the desired specificity into `out`.
pub fn write_event_type_value(
    out: &mut Vec<u8>,
    type_address: &[u8],
    module: Option<&str>,
    name: Option<&str>,
    instantiation_bcs: Option<&[u8]>,
) {
    out.clear();
    out.reserve(32 + 32);
    out.extend_from_slice(type_address);
    if let Some(m) = module {
        out.extend_from_slice(m.as_bytes());
        if let Some(n) = name {
            out.push(0x00);
            out.extend_from_slice(n.as_bytes());
            if let Some(bcs) = instantiation_bcs {
                out.push(0x00);
                out.extend_from_slice(bcs);
            }
        }
    }
}

/// Build an EventType compound value at the desired specificity.
/// `instantiation_bcs` is the BCS encoding of `Vec<TypeTag>`, used only
/// when matching a fully instantiated generic type.
pub fn event_type_value(
    type_address: &[u8],
    module: Option<&str>,
    name: Option<&str>,
    instantiation_bcs: Option<&[u8]>,
) -> Vec<u8> {
    let mut v = Vec::with_capacity(32 + 32);
    write_event_type_value(&mut v, type_address, module, name, instantiation_bcs);
    v
}

/// Visit all tx-space dimensions for a transaction.
///
/// The callback is invoked once per logical tx-space dimension candidate.
/// Compound dimensions are emitted at every prefix level so queries at any
/// specificity remain a single key lookup.
pub fn for_each_transaction_dimension(
    tx: &ExecutedTransaction,
    mut f: impl FnMut(IndexDimension, &[u8]),
) {
    let mut scratch = Vec::new();

    f(IndexDimension::Sender, tx.transaction.sender().as_ref());

    for (_, owner, _) in tx.effects.all_changed_objects() {
        match owner {
            Owner::AddressOwner(addr) => f(IndexDimension::Recipient, addr.as_ref()),
            Owner::ConsensusAddressOwner { owner, .. } => {
                f(IndexDimension::Recipient, owner.as_ref())
            }
            _ => {}
        }
    }

    for change in tx.effects.object_changes() {
        f(IndexDimension::AffectedObject, change.id.as_ref());
    }

    for (_, package_id, module, function) in tx.transaction.move_calls() {
        let pkg = package_id.as_ref();

        write_move_call_value(&mut scratch, pkg, None, None);
        f(IndexDimension::MoveCall, &scratch);

        write_move_call_value(&mut scratch, pkg, Some(module), None);
        f(IndexDimension::MoveCall, &scratch);

        write_move_call_value(&mut scratch, pkg, Some(module), Some(function));
        f(IndexDimension::MoveCall, &scratch);
    }

    for ev in tx.events.iter().flat_map(|evs| evs.data.iter()) {
        let pkg = ev.package_id.as_ref();
        let type_addr = ev.type_.address.as_ref();
        let emit_mod: &str = ev.transaction_module.as_str();
        let type_mod: &str = ev.type_.module.as_str();
        let type_name: &str = ev.type_.name.as_str();

        write_emit_module_value(&mut scratch, pkg, None);
        f(IndexDimension::EmitModule, &scratch);

        write_emit_module_value(&mut scratch, pkg, Some(emit_mod));
        f(IndexDimension::EmitModule, &scratch);

        write_event_type_value(&mut scratch, type_addr, None, None, None);
        f(IndexDimension::EventType, &scratch);

        write_event_type_value(&mut scratch, type_addr, Some(type_mod), None, None);
        f(IndexDimension::EventType, &scratch);

        write_event_type_value(
            &mut scratch,
            type_addr,
            Some(type_mod),
            Some(type_name),
            None,
        );
        f(IndexDimension::EventType, &scratch);

        if !ev.type_.type_params.is_empty() {
            let params_bcs =
                bcs::to_bytes(&ev.type_.type_params).expect("BCS encoding of type params");

            write_event_type_value(
                &mut scratch,
                type_addr,
                Some(type_mod),
                Some(type_name),
                Some(&params_bcs),
            );
            f(IndexDimension::EventType, &scratch);
        }
    }
}

/// Visit all event-space dimensions for a transaction.
///
/// The callback receives `(event_idx, dimension, value)` once per logical
/// event-space dimension candidate.
pub fn for_each_event_dimension(
    tx: &ExecutedTransaction,
    mut f: impl FnMut(u32, IndexDimension, &[u8]),
) {
    let mut scratch = Vec::new();
    let sender = tx.transaction.sender();

    for (idx, ev) in tx.events.iter().flat_map(|evs| evs.data.iter()).enumerate() {
        let event_idx = idx as u32;

        f(event_idx, IndexDimension::Sender, sender.as_ref());

        let pkg = ev.package_id.as_ref();
        let type_addr = ev.type_.address.as_ref();
        let emit_mod: &str = ev.transaction_module.as_str();
        let type_mod: &str = ev.type_.module.as_str();
        let type_name: &str = ev.type_.name.as_str();

        write_emit_module_value(&mut scratch, pkg, None);
        f(event_idx, IndexDimension::EmitModule, &scratch);

        write_emit_module_value(&mut scratch, pkg, Some(emit_mod));
        f(event_idx, IndexDimension::EmitModule, &scratch);

        write_event_type_value(&mut scratch, type_addr, None, None, None);
        f(event_idx, IndexDimension::EventType, &scratch);

        write_event_type_value(&mut scratch, type_addr, Some(type_mod), None, None);
        f(event_idx, IndexDimension::EventType, &scratch);

        write_event_type_value(
            &mut scratch,
            type_addr,
            Some(type_mod),
            Some(type_name),
            None,
        );
        f(event_idx, IndexDimension::EventType, &scratch);

        if !ev.type_.type_params.is_empty() {
            let params_bcs =
                bcs::to_bytes(&ev.type_.type_params).expect("BCS encoding of type params");
            write_event_type_value(
                &mut scratch,
                type_addr,
                Some(type_mod),
                Some(type_name),
                Some(&params_bcs),
            );
            f(event_idx, IndexDimension::EventType, &scratch);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dimension_tags_are_unique() {
        use std::collections::HashSet;
        let tags = [
            IndexDimension::Sender,
            IndexDimension::Recipient,
            IndexDimension::AffectedObject,
            IndexDimension::MoveCall,
            IndexDimension::EmitModule,
            IndexDimension::EventType,
        ];
        let tag_bytes: HashSet<u8> = tags.iter().map(|t| t.tag_byte()).collect();
        assert_eq!(
            tag_bytes.len(),
            tags.len(),
            "all dimension tags must be unique"
        );
    }

    #[test]
    fn test_encode_dimension_key_format() {
        let value = b"hello";
        let key = encode_dimension_key(IndexDimension::EmitModule, value);
        assert_eq!(key[0], 0x05);
        assert_eq!(&key[1..], b"hello");
    }

    #[test]
    fn test_encode_dimension_key_no_collision() {
        let value = vec![0x42; 32];
        let key1 = encode_dimension_key(IndexDimension::Sender, &value);
        let key2 = encode_dimension_key(IndexDimension::Recipient, &value);
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_move_call_compound_key_hierarchy() {
        let pkg = [0xAA; 32];

        let pkg_only = move_call_value(&pkg, None, None);
        let pkg_mod = move_call_value(&pkg, Some("coin"), None);
        let pkg_mod_func = move_call_value(&pkg, Some("coin"), Some("transfer"));

        // Package-level is just the 32-byte address
        assert_eq!(pkg_only.len(), 32);
        // Module-level extends with module bytes
        assert_eq!(pkg_mod.len(), 32 + 4);
        // Function-level adds \x00 separator + function bytes
        assert_eq!(pkg_mod_func.len(), 32 + 4 + 1 + 8);
        assert_eq!(pkg_mod_func[36], 0x00);

        // Each level is a strict prefix of the next
        assert!(pkg_mod.starts_with(&pkg_only));
        assert!(pkg_mod_func.starts_with(&pkg_mod[..36]));
    }

    #[test]
    fn test_move_call_no_collision_across_modules() {
        let pkg = [0xBB; 32];
        // "ab" + function "cd" vs "a" + function "bcd"
        let key1 = move_call_value(&pkg, Some("ab"), Some("cd"));
        let key2 = move_call_value(&pkg, Some("a"), Some("bcd"));
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_event_type_compound_key_hierarchy() {
        let addr = [0xCC; 32];

        let addr_only = event_type_value(&addr, None, None, None);
        let addr_mod = event_type_value(&addr, Some("coin"), None, None);
        let addr_mod_name = event_type_value(&addr, Some("coin"), Some("CoinEvent"), None);
        let bcs_params = vec![0x01, 0x02, 0x03];
        let addr_full = event_type_value(&addr, Some("coin"), Some("CoinEvent"), Some(&bcs_params));

        assert_eq!(addr_only.len(), 32);
        assert_eq!(addr_mod.len(), 32 + 4);
        assert_eq!(addr_mod_name.len(), 32 + 4 + 1 + 9);
        assert_eq!(addr_full.len(), 32 + 4 + 1 + 9 + 1 + 3);

        // Separators at the right positions
        assert_eq!(addr_mod_name[36], 0x00); // between module and name
        assert_eq!(addr_full[46], 0x00); // between name and instantiation
    }

    #[test]
    fn test_emit_module_compound_key() {
        let pkg = [0xDD; 32];

        let pkg_only = emit_module_value(&pkg, None);
        let pkg_mod = emit_module_value(&pkg, Some("transfer"));

        assert_eq!(pkg_only.len(), 32);
        assert_eq!(pkg_mod.len(), 32 + 8);
        assert!(pkg_mod.starts_with(&pkg_only));
    }
}
