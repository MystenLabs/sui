// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Transactions table: stores transaction data indexed by digest.

use anyhow::{Context, Result};
use bytes::Bytes;
use sui_types::balance_change::BalanceChange;
use sui_types::digests::TransactionDigest;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::signature::GenericSignature;
use sui_types::storage::ObjectKey;
use sui_types::transaction::{SenderSignedData, Transaction};

use crate::write_legacy_data;
use crate::{TransactionData, TransactionEventsData};

pub mod col {
    pub const EFFECTS: &str = "ef";
    pub const EVENTS: &str = "ev";
    pub const TIMESTAMP: &str = "ts";
    pub const CHECKPOINT_NUMBER: &str = "cn";
    pub const DATA: &str = "td";
    pub const SIGNATURES: &str = "sg";
    pub const BALANCE_CHANGES: &str = "bc";
    pub const UNCHANGED_LOADED: &str = "ul";
    /// Deprecated: Full Transaction (data+sigs combined). Use DATA+SIGNATURES instead.
    pub const TX: &str = "tx";
}

pub const NAME: &str = "transactions";

pub fn encode_key(digest: &TransactionDigest) -> Vec<u8> {
    digest.inner().to_vec()
}

/// Encode all transaction columns.
/// Writes 8 columns by default (or 9 when `write_legacy_data()` is enabled, which adds
/// the deprecated TX column).
pub fn encode(
    transaction: &Transaction,
    effects: &TransactionEffects,
    events: &Option<TransactionEvents>,
    checkpoint_number: CheckpointSequenceNumber,
    timestamp_ms: u64,
    balance_changes: &[BalanceChange],
    unchanged_loaded_runtime_objects: &[ObjectKey],
) -> Result<Vec<(&'static str, Bytes)>> {
    let mut cols = Vec::with_capacity(if write_legacy_data() { 9 } else { 8 });

    if write_legacy_data() {
        cols.push((col::TX, Bytes::from(bcs::to_bytes(transaction)?)));
    }

    cols.extend([
        (col::EFFECTS, Bytes::from(bcs::to_bytes(effects)?)),
        (col::EVENTS, Bytes::from(bcs::to_bytes(events)?)),
        (col::TIMESTAMP, Bytes::from(bcs::to_bytes(&timestamp_ms)?)),
        (
            col::CHECKPOINT_NUMBER,
            Bytes::from(bcs::to_bytes(&checkpoint_number)?),
        ),
        (
            col::DATA,
            Bytes::from(bcs::to_bytes(&transaction.data().intent_message().value)?),
        ),
        (
            col::SIGNATURES,
            Bytes::from(bcs::to_bytes(transaction.data().tx_signatures())?),
        ),
        (
            col::BALANCE_CHANGES,
            Bytes::from(bcs::to_bytes(balance_changes)?),
        ),
        (
            col::UNCHANGED_LOADED,
            Bytes::from(bcs::to_bytes(unchanged_loaded_runtime_objects)?),
        ),
    ]);

    Ok(cols)
}

pub fn decode(row: &[(Bytes, Bytes)]) -> Result<TransactionData> {
    let mut tx_data: Option<sui_types::transaction::TransactionData> = None;
    let mut tx_signatures: Option<Vec<GenericSignature>> = None;

    let mut effects = None;
    let mut events = None;
    let mut timestamp = 0;
    let mut checkpoint_number = 0;
    let mut balance_changes = Vec::new();
    let mut unchanged_loaded_runtime_objects = Vec::new();

    for (column, value) in row {
        match column.as_ref() {
            b"td" => tx_data = Some(bcs::from_bytes(value)?),
            b"sg" => tx_signatures = Some(bcs::from_bytes(value)?),
            b"ef" => effects = Some(bcs::from_bytes(value)?),
            b"ev" => events = Some(bcs::from_bytes(value)?),
            b"ts" => timestamp = bcs::from_bytes(value)?,
            b"cn" => checkpoint_number = bcs::from_bytes(value)?,
            b"bc" => balance_changes = bcs::from_bytes(value)?,
            b"ul" => unchanged_loaded_runtime_objects = bcs::from_bytes(value)?,
            _ => {}
        }
    }

    let transaction = {
        let data = tx_data.context("transaction data field is missing")?;
        let sigs = tx_signatures.context("transaction signatures field is missing")?;
        let sender_signed_data = SenderSignedData::new(data, sigs);
        Transaction::new(sender_signed_data)
    };

    Ok(TransactionData {
        transaction,
        effects: effects.context("effects field is missing")?,
        events: events.context("events field is missing")?,
        timestamp,
        checkpoint_number,
        balance_changes,
        unchanged_loaded_runtime_objects,
    })
}

/// Decode only events and timestamp from a row (for partial reads).
pub fn decode_events(row: &[(Bytes, Bytes)]) -> Result<TransactionEventsData> {
    let mut transaction_events: Option<Option<TransactionEvents>> = None;
    let mut timestamp_ms = 0;

    for (column, value) in row {
        match column.as_ref() {
            b"ev" => transaction_events = Some(bcs::from_bytes(value)?),
            b"ts" => timestamp_ms = bcs::from_bytes(value)?,
            _ => {}
        }
    }

    let events = transaction_events
        .context("events field is missing")?
        .map(|e| e.data)
        .unwrap_or_default();

    Ok(TransactionEventsData {
        events,
        timestamp_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use sui_types::TypeTag;
    use sui_types::balance_change::BalanceChange;
    use sui_types::base_types::{ObjectID, SuiAddress};
    use sui_types::effects::TestEffectsBuilder;
    use sui_types::object::Object;
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
    use sui_types::storage::ObjectKey;
    use sui_types::transaction::{SenderSignedData, Transaction, TransactionData};

    fn test_transaction() -> Transaction {
        let sender = SuiAddress::random_for_testing_only();
        let gas = Object::immutable_with_id_for_testing(ObjectID::random());
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder.transfer_sui(SuiAddress::random_for_testing_only(), None);
            builder.finish()
        };
        let data = TransactionData::new_programmable(
            sender,
            vec![gas.compute_object_reference()],
            pt,
            1_000_000,
            1,
        );
        Transaction::new(SenderSignedData::new(data, vec![]))
    }

    #[test]
    fn encode_decode_round_trip() {
        let transaction = test_transaction();
        let effects = TestEffectsBuilder::new(transaction.data()).build();
        let balance_change = BalanceChange {
            address: SuiAddress::random_for_testing_only(),
            coin_type: TypeTag::U64,
            amount: 42,
        };
        let obj_key = ObjectKey(ObjectID::random(), 3.into());

        let encoded = encode(
            &transaction,
            &effects,
            &None,
            7,
            42,
            std::slice::from_ref(&balance_change),
            std::slice::from_ref(&obj_key),
        )
        .expect("encoding should succeed");
        let row: Vec<(Bytes, Bytes)> = encoded
            .into_iter()
            .map(|(column, value)| (Bytes::from_static(column.as_bytes()), value))
            .collect();

        let decoded = decode(&row).expect("decoding should succeed");

        assert_eq!(decoded.balance_changes, vec![balance_change]);
        assert_eq!(decoded.unchanged_loaded_runtime_objects, vec![obj_key]);
    }
}
