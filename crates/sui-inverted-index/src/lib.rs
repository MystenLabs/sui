// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::accumulator_root::stream_id_from_accumulator_event;
use sui_types::balance::Balance;
use sui_types::effects::AccumulatorValue;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::ExecutedTransaction;
use sui_types::full_checkpoint_content::ObjectSet;
use sui_types::object::Owner;
use sui_types::storage::ObjectKey;
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
    /// Any address whose state moved as a side effect of the transaction:
    /// addresses that own an object after the txn (transfers in / new
    /// owned objects), addresses that owned an object before the txn that
    /// was mutated, transferred away, deleted, or wrapped, and addresses
    /// whose address-balance changed via an accumulator `Balance<T>` event.
    AffectedAddress = 0x02,
    AffectedObject = 0x03,
    /// Compound: `[package_32]` | `[package_32][module]` | `[package_32][module\x00function]`
    MoveCall = 0x04,
    /// Compound: `[package_id_32]` | `[package_id_32][module]`
    EmitModule = 0x05,
    /// Compound: `[type_address_32]` | `[..][module]` | `[..\x00name]` | `[..\x00name\x00instantiation_bcs]`
    EventType = 0x06,
    /// Authenticated event stream head id (the `SuiAddress` keying an
    /// `accumulator_settlement::EventStreamHead` accumulator). Tx-space matches
    /// any transaction that wrote to the stream; event-space matches the
    /// individual events committed to the stream.
    EventStreamHead = 0x07,
}

impl IndexDimension {
    pub fn tag_byte(self) -> u8 {
        self as u8
    }
}

const COMPOUND_VALUE_SEPARATOR: u8 = 0x00;

/// Visit all tx-space dimensions for a transaction.
///
/// `object_set` is used to resolve owners for the input and output states
/// referenced by `tx.effects.object_changes()`. Callers typically pass
/// `&checkpoint.object_set`.
///
/// The callback is invoked once per logical tx-space dimension candidate as
/// `f(dimension, key)`, where `key` is the encoded value bytes for that
/// dimension (the lookup key, without the dimension's tag byte prefix).
/// Compound dimensions are emitted at every prefix level so queries at any
/// specificity remain a single key lookup.
///
/// This uses a visitor rather than returning owned dimension values so the
/// extractor can reuse one scratch buffer for all compound keys emitted from a
/// transaction instead of allocating one `Vec<u8>` per dimension.
pub fn for_each_transaction_dimension(
    tx: &ExecutedTransaction,
    object_set: &ObjectSet,
    mut f: impl FnMut(IndexDimension, &[u8]),
) {
    let mut scratch = Vec::new();

    f(IndexDimension::Sender, tx.transaction.sender().as_ref());

    for change in tx.effects.object_changes() {
        for version in [change.input_version, change.output_version]
            .into_iter()
            .flatten()
        {
            let Some(obj) = object_set.get(&ObjectKey(change.id, version)) else {
                continue;
            };
            if let Some(addr) = owner_as_affected_address(obj.owner()) {
                f(IndexDimension::AffectedAddress, addr);
            }
        }

        f(IndexDimension::AffectedObject, change.id.as_ref());
    }

    for (_, package_id, module, function) in tx.transaction.move_calls() {
        let pkg = package_id.as_ref();

        scratch.clear();
        scratch.reserve(pkg.len() + module.len() + 1 + function.len());
        append_dimension_value_component(&mut scratch, pkg);
        f(IndexDimension::MoveCall, &scratch);

        append_dimension_value_component(&mut scratch, module.as_bytes());
        f(IndexDimension::MoveCall, &scratch);

        append_separated_dimension_value_component(&mut scratch, function.as_bytes());
        f(IndexDimension::MoveCall, &scratch);
    }

    for_each_event_dimension(tx, |_idx, dim, key| f(dim, key));

    for acc in tx.effects.accumulator_events() {
        if Balance::is_balance_type(&acc.write.address.ty)
            && matches!(&acc.write.value, AccumulatorValue::Integer(_))
        {
            f(
                IndexDimension::AffectedAddress,
                acc.write.address.address.as_ref(),
            );
        }
    }
}

/// Visit all event-space dimensions for a transaction.
///
/// The callback is invoked as `f(event_idx, dimension, key)` once per logical
/// event-space dimension candidate, where `key` is the encoded value bytes
/// for that dimension (the lookup key, without the dimension's tag byte
/// prefix).
///
/// Like [`for_each_transaction_dimension`], this keeps the ownership boundary
/// inside the visitor call so compound event keys can borrow a reused scratch
/// buffer while the caller consumes each value synchronously.
pub fn for_each_event_dimension(
    tx: &ExecutedTransaction,
    mut f: impl FnMut(u32, IndexDimension, &[u8]),
) {
    let mut scratch = Vec::new();
    let event_count = tx.events.as_ref().map(|e| e.data.len()).unwrap_or(0);

    for (idx, ev) in tx.events.iter().flat_map(|evs| evs.data.iter()).enumerate() {
        let event_idx = u32::try_from(idx).expect("event index exceeds u32::MAX");

        let pkg = ev.package_id.as_ref();
        let type_addr = ev.type_.address.as_ref();
        let emit_mod: &str = ev.transaction_module.as_str();
        let type_mod: &str = ev.type_.module.as_str();
        let type_name: &str = ev.type_.name.as_str();

        scratch.clear();
        scratch.reserve(pkg.len() + emit_mod.len());
        append_dimension_value_component(&mut scratch, pkg);
        f(event_idx, IndexDimension::EmitModule, &scratch);

        append_dimension_value_component(&mut scratch, emit_mod.as_bytes());
        f(event_idx, IndexDimension::EmitModule, &scratch);

        scratch.clear();
        scratch.reserve(type_addr.len() + type_mod.len() + type_name.len() + 2);
        append_dimension_value_component(&mut scratch, type_addr);
        f(event_idx, IndexDimension::EventType, &scratch);

        append_dimension_value_component(&mut scratch, type_mod.as_bytes());
        f(event_idx, IndexDimension::EventType, &scratch);

        append_separated_dimension_value_component(&mut scratch, type_name.as_bytes());
        f(event_idx, IndexDimension::EventType, &scratch);

        if !ev.type_.type_params.is_empty() {
            let params_bcs =
                bcs::to_bytes(&ev.type_.type_params).expect("BCS encoding of type params");
            append_separated_dimension_value_component(&mut scratch, &params_bcs);
            f(event_idx, IndexDimension::EventType, &scratch);
        }
    }

    for acc in tx.effects.accumulator_events() {
        let AccumulatorValue::EventDigest(event_digests) = &acc.write.value else {
            continue;
        };
        let Some(stream_id) = stream_id_from_accumulator_event(&acc) else {
            continue;
        };
        for (idx, _digest) in event_digests {
            let event_idx = u32::try_from(*idx).expect("accumulator event index exceeds u32::MAX");
            assert!(
                (*idx as usize) < event_count,
                "accumulator event references event idx {} but txn emitted only {} events",
                idx,
                event_count,
            );
            f(
                event_idx,
                IndexDimension::EventStreamHead,
                stream_id.as_ref(),
            );
        }
    }
}

/// Encode a dimension value into a row key component: `[tag_byte][value_bytes]`.
pub fn encode_dimension_key(dim: IndexDimension, value: &[u8]) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + value.len());
    write_dimension_key(&mut key, dim, value);
    key
}

/// Build a MoveCall compound value at the desired specificity.
pub fn move_call_value(package: &[u8], module: Option<&str>, function: Option<&str>) -> Vec<u8> {
    let mut v = Vec::with_capacity(32 + 32);
    write_move_call_value(&mut v, package, module, function);
    v
}

/// Build an EmitModule compound value at the desired specificity.
pub fn emit_module_value(package_id: &[u8], module: Option<&str>) -> Vec<u8> {
    let mut v = Vec::with_capacity(32 + 16);
    write_emit_module_value(&mut v, package_id, module);
    v
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

/// Append a dimension key into `out` using the `[tag_byte][value_bytes]` format.
pub fn write_dimension_key(out: &mut Vec<u8>, dim: IndexDimension, value: &[u8]) {
    out.clear();
    out.reserve(1 + value.len());
    out.push(dim.tag_byte());
    out.extend_from_slice(value);
}

/// Append one component to a compound dimension value.
pub fn append_dimension_value_component(out: &mut Vec<u8>, component: &[u8]) {
    out.extend_from_slice(component);
}

/// Append one separator-prefixed component to a compound dimension value.
pub fn append_separated_dimension_value_component(out: &mut Vec<u8>, component: &[u8]) {
    out.push(COMPOUND_VALUE_SEPARATOR);
    append_dimension_value_component(out, component);
}

/// Append a MoveCall compound value at the desired specificity into `out`.
pub fn write_move_call_value(
    out: &mut Vec<u8>,
    package: &[u8],
    module: Option<&str>,
    function: Option<&str>,
) {
    out.clear();
    out.reserve(32 + 32);
    append_dimension_value_component(out, package);
    if let Some(m) = module {
        append_dimension_value_component(out, m.as_bytes());
        if let Some(f) = function {
            append_separated_dimension_value_component(out, f.as_bytes());
        }
    }
}

/// Append an EmitModule compound value at the desired specificity into `out`.
pub fn write_emit_module_value(out: &mut Vec<u8>, package_id: &[u8], module: Option<&str>) {
    out.clear();
    out.reserve(32 + 16);
    append_dimension_value_component(out, package_id);
    if let Some(m) = module {
        append_dimension_value_component(out, m.as_bytes());
    }
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
    append_dimension_value_component(out, type_address);
    if let Some(m) = module {
        append_dimension_value_component(out, m.as_bytes());
        if let Some(n) = name {
            append_separated_dimension_value_component(out, n.as_bytes());
            if let Some(bcs) = instantiation_bcs {
                append_separated_dimension_value_component(out, bcs);
            }
        }
    }
}

fn owner_as_affected_address(owner: &Owner) -> Option<&[u8]> {
    match owner {
        Owner::AddressOwner(addr) => Some(addr.as_ref()),
        Owner::ConsensusAddressOwner { owner, .. } => Some(owner.as_ref()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use move_core_types::ident_str;
    use sui_types::accumulator_event::AccumulatorEvent;
    use sui_types::base_types::ObjectID;
    use sui_types::effects::TestEffectsBuilder;
    use sui_types::event::Event;
    use sui_types::gas_coin::GAS;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;
    use sui_types::transaction::SenderSignedData;

    use super::*;

    #[test]
    fn transaction_visitor_emits_tx_and_event_dimensions() {
        let sender = TestCheckpointBuilder::derive_address(1);
        let recipient = TestCheckpointBuilder::derive_address(2);
        let affected = TestCheckpointBuilder::derive_object_id(10);
        let package = ObjectID::ZERO;
        let event_type = GAS::type_();
        let checkpoint = TestCheckpointBuilder::new(0)
            .start_transaction(1)
            .create_coin_object(10, 2, 100, GAS::type_tag())
            .add_move_call(package, "coin", "transfer")
            .with_events(vec![Event::new(
                &package,
                ident_str!("emit_mod"),
                sender,
                event_type.clone(),
                vec![],
            )])
            .finish_transaction()
            .build_checkpoint();
        let tx = &checkpoint.transactions[0];

        let mut keys = HashSet::new();
        for_each_transaction_dimension(tx, &checkpoint.object_set, |dim, value| {
            keys.insert(encode_dimension_key(dim, value));
        });

        assert!(keys.contains(&encode_dimension_key(
            IndexDimension::Sender,
            sender.as_ref()
        )));
        assert!(keys.contains(&encode_dimension_key(
            IndexDimension::AffectedAddress,
            recipient.as_ref()
        )));
        assert!(keys.contains(&encode_dimension_key(
            IndexDimension::AffectedObject,
            affected.as_ref()
        )));
        assert!(keys.contains(&encode_dimension_key(
            IndexDimension::MoveCall,
            &move_call_value(package.as_ref(), Some("coin"), Some("transfer"))
        )));
        assert!(keys.contains(&encode_dimension_key(
            IndexDimension::EmitModule,
            &emit_module_value(package.as_ref(), Some("emit_mod"))
        )));
        assert!(keys.contains(&encode_dimension_key(
            IndexDimension::EventType,
            &event_type_value(
                event_type.address.as_ref(),
                Some(event_type.module.as_str()),
                Some(event_type.name.as_str()),
                None,
            )
        )));
    }

    #[test]
    fn event_visitor_emits_event_dimensions_per_event() {
        let sender = TestCheckpointBuilder::derive_address(1);
        let affected = TestCheckpointBuilder::derive_object_id(10);
        let package = ObjectID::ZERO;
        let event_type = GAS::type_();
        let checkpoint = TestCheckpointBuilder::new(0)
            .start_transaction(1)
            .create_coin_object(10, 2, 100, GAS::type_tag())
            .add_move_call(package, "coin", "transfer")
            .with_events(vec![Event::new(
                &package,
                ident_str!("emit_mod"),
                sender,
                event_type.clone(),
                vec![],
            )])
            .finish_transaction()
            .build_checkpoint();
        let tx = &checkpoint.transactions[0];

        let mut keys = HashSet::new();
        for_each_event_dimension(tx, |event_idx, dim, value| {
            keys.insert((event_idx, encode_dimension_key(dim, value)));
        });

        for expected in [
            encode_dimension_key(
                IndexDimension::EmitModule,
                &emit_module_value(package.as_ref(), Some("emit_mod")),
            ),
            encode_dimension_key(
                IndexDimension::EventType,
                &event_type_value(
                    event_type.address.as_ref(),
                    Some(event_type.module.as_str()),
                    Some(event_type.name.as_str()),
                    None,
                ),
            ),
        ] {
            assert!(keys.contains(&(0, expected)));
        }

        let move_call_key = encode_dimension_key(
            IndexDimension::MoveCall,
            &move_call_value(package.as_ref(), Some("coin"), Some("transfer")),
        );
        let sender_key = encode_dimension_key(IndexDimension::Sender, sender.as_ref());
        let affected_object_key =
            encode_dimension_key(IndexDimension::AffectedObject, affected.as_ref());

        assert!(!keys.iter().any(|(_, k)| k == &sender_key));
        assert!(!keys.iter().any(|(_, k)| k == &affected_object_key));
        assert!(!keys.iter().any(|(_, k)| k == &move_call_key));
    }

    #[test]
    fn affected_address_captures_prior_owner_on_transfer() {
        let alice = TestCheckpointBuilder::derive_address(1);
        let bob = TestCheckpointBuilder::derive_address(2);
        let checkpoint = TestCheckpointBuilder::new(0)
            .start_transaction(1)
            .create_owned_object(10)
            .finish_transaction()
            .start_transaction(1)
            .transfer_object(10, 2)
            .finish_transaction()
            .build_checkpoint();
        let transfer_tx = &checkpoint.transactions[1];

        let mut keys = HashSet::new();
        for_each_transaction_dimension(transfer_tx, &checkpoint.object_set, |dim, value| {
            keys.insert(encode_dimension_key(dim, value));
        });

        assert!(
            keys.contains(&encode_dimension_key(
                IndexDimension::AffectedAddress,
                bob.as_ref()
            )),
            "new owner Bob should be captured via object_changes output state"
        );
        assert!(
            keys.contains(&encode_dimension_key(
                IndexDimension::AffectedAddress,
                alice.as_ref()
            )),
            "prior owner Alice should be captured via object_changes input state"
        );
    }

    #[test]
    fn affected_address_captures_address_balance_accumulator() {
        let balance_owner = TestCheckpointBuilder::derive_address(2);
        let mut checkpoint = TestCheckpointBuilder::new(0)
            .start_transaction(1)
            .finish_transaction()
            .build_checkpoint();
        let tx = &checkpoint.transactions[0];
        let signed = SenderSignedData::new(tx.transaction.clone(), tx.signatures.clone());
        let accumulator_event = AccumulatorEvent::from_balance_change(
            balance_owner,
            Balance::type_tag(GAS::type_tag()),
            100,
        )
        .unwrap();
        checkpoint.transactions[0].effects = TestEffectsBuilder::new(&signed)
            .with_accumulator_events([accumulator_event])
            .build();
        let tx = &checkpoint.transactions[0];

        let mut keys = HashSet::new();
        for_each_transaction_dimension(tx, &checkpoint.object_set, |dim, value| {
            keys.insert(encode_dimension_key(dim, value));
        });

        assert!(keys.contains(&encode_dimension_key(
            IndexDimension::AffectedAddress,
            balance_owner.as_ref()
        )));
    }
}
