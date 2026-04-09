// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::TypeTag;
use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;

use sui_types::base_types::{EpochId, SequenceNumber, SuiAddress};
use sui_types::digests::ChainIdentifier;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::*;
use sui_types::type_input::{StructInput, TypeInput};
use sui_types::{SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION};

#[derive(Clone, Debug)]
pub struct TxFuzzContext {
    pub sender: SuiAddress,
    pub chain: ChainIdentifier,
    pub epoch: EpochId,
    pub reference_gas_price: u64,
    /// Fund type used for FundsWithdrawal and PT-input coin reservations.
    pub fund_type: Arc<TypeTag>,
    /// Optional sponsor address (must be funded). When set, some strategies will
    /// produce sponsored transactions where `gas_data.owner = sponsor`. The test
    /// driver detects `sender != gas_owner` and dual-signs accordingly.
    pub sponsor: Option<SuiAddress>,
    /// Initial shared version of the `0x8` randomness state object on the live
    /// cluster. When `Some`, the valid PT building blocks may emit a randomness
    /// reference; when `None`, the randomness block is skipped.
    pub randomness_initial_shared_version: Option<SequenceNumber>,
}

pub(super) fn boundary_u64() -> BoxedStrategy<u64> {
    prop_oneof![
        3 => any::<u64>(),
        1 => Just(0u64),
        1 => Just(1u64),
        1 => Just(u64::MAX),
        1 => Just(u64::MAX - 1),
    ]
    .boxed()
}

pub(super) fn type_input_strategy() -> BoxedStrategy<TypeInput> {
    let leaf = prop_oneof![
        Just(TypeInput::Bool),
        Just(TypeInput::U8),
        Just(TypeInput::U16),
        Just(TypeInput::U32),
        Just(TypeInput::U64),
        Just(TypeInput::U128),
        Just(TypeInput::U256),
        Just(TypeInput::Address),
        Just(TypeInput::Signer),
    ];
    leaf.prop_recursive(4, 16, 4, |inner| {
        prop_oneof![
            inner.clone().prop_map(|t| TypeInput::Vector(Box::new(t))),
            (
                any::<AccountAddress>(),
                "[a-z]{1,8}".prop_map(String::from),
                "[a-z]{1,8}".prop_map(String::from),
                vec(inner, 0..3),
            )
                .prop_map(|(address, module, name, type_params)| {
                    TypeInput::Struct(Box::new(StructInput {
                        address,
                        module,
                        name,
                        type_params,
                    }))
                }),
        ]
    })
    .boxed()
}

pub(super) fn simple_transfer_pt(sender: SuiAddress) -> ProgrammableTransaction {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .obj(ObjectArg::SharedObject {
            id: SUI_CLOCK_OBJECT_ID,
            initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
            mutability: SharedObjectMutability::Immutable,
        })
        .unwrap();
    builder.transfer_sui(sender, None);
    builder.finish()
}

/// Shared expiration strategy. Mostly generates valid `ValidDuring` for the current
/// epoch+chain, with boundary variants probing each field.
pub(super) fn expiration_strategy(ctx: &TxFuzzContext) -> BoxedStrategy<TransactionExpiration> {
    let chain = ctx.chain;
    let epoch = ctx.epoch;
    prop_oneof![
        12 => any::<u32>().prop_map(move |nonce| TransactionExpiration::ValidDuring {
            min_epoch: Some(epoch),
            max_epoch: Some(epoch),
            min_timestamp: None,
            max_timestamp: None,
            chain,
            nonce,
        }),
        1 => Just(TransactionExpiration::None),
        1 => boundary_u64().prop_map(TransactionExpiration::Epoch),
        1 => (any::<ChainIdentifier>(), any::<u32>()).prop_map(move |(c, nonce)| {
            TransactionExpiration::ValidDuring {
                min_epoch: Some(epoch),
                max_epoch: Some(epoch),
                min_timestamp: None,
                max_timestamp: None,
                chain: c,
                nonce,
            }
        }),
        1 => (boundary_u64(), any::<u32>()).prop_map(move |(ts, nonce)| {
            TransactionExpiration::ValidDuring {
                min_epoch: Some(epoch),
                max_epoch: Some(epoch),
                min_timestamp: Some(ts),
                max_timestamp: None,
                chain,
                nonce,
            }
        }),
        1 => (boundary_u64(), any::<u32>()).prop_map(move |(e, nonce)| {
            TransactionExpiration::ValidDuring {
                min_epoch: Some(e),
                max_epoch: Some(e),
                min_timestamp: None,
                max_timestamp: None,
                chain,
                nonce,
            }
        }),
        1 => (
            option::of(boundary_u64()),
            option::of(boundary_u64()),
            option::of(boundary_u64()),
            option::of(boundary_u64()),
            any::<ChainIdentifier>(),
            any::<u32>(),
        ).prop_map(|(min_epoch, max_epoch, min_timestamp, max_timestamp, c, nonce)| {
            TransactionExpiration::ValidDuring {
                min_epoch, max_epoch, min_timestamp, max_timestamp, chain: c, nonce,
            }
        }),
    ]
    .boxed()
}
