// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use proptest::arbitrary::*;
use proptest::prelude::*;

use crate::type_arg_fuzzer::{gen_type_tag, pt_for_tags};
use proptest::collection::vec;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};

use sui_types::digests::ObjectDigest;
use sui_types::messages::{
    GasData, TransactionData, TransactionDataV1, TransactionExpiration, TransactionKind,
};

use crate::account_universe::{gas_budget_selection_strategy, gas_price_selection_strategy};

const MAX_NUM_GAS_OBJS: usize = 1024_usize;

pub fn gen_transaction_expiration_with_bound(
    max_epoch: u64,
) -> impl Strategy<Value = TransactionExpiration> {
    prop_oneof![
        Just(TransactionExpiration::None),
        (0u64..=max_epoch).prop_map(TransactionExpiration::Epoch),
    ]
}

pub fn gen_transaction_expiration() -> impl Strategy<Value = TransactionExpiration> {
    prop_oneof![
        Just(TransactionExpiration::None),
        (0u64..=u64::MAX).prop_map(TransactionExpiration::Epoch),
    ]
}

pub fn gen_object_ref() -> impl Strategy<Value = ObjectRef> {
    (
        any::<AccountAddress>(),
        any::<SequenceNumber>(),
        any::<[u8; 32]>(),
    )
        .prop_map(move |(addr, seq, seed)| {
            (ObjectID::from_address(addr), seq, ObjectDigest::new(seed))
        })
}

pub fn gen_gas_data(sender: SuiAddress) -> impl Strategy<Value = GasData> {
    (
        vec(gen_object_ref(), 0..MAX_NUM_GAS_OBJS),
        gas_price_selection_strategy(),
        gas_budget_selection_strategy(),
    )
        .prop_map(move |(obj_refs, price, budget)| GasData {
            payment: obj_refs,
            owner: sender,
            price,
            budget,
        })
}

pub fn gen_transaction_kind() -> impl Strategy<Value = TransactionKind> {
    (vec(gen_type_tag(), 0..10))
        .prop_map(pt_for_tags)
        .prop_map(TransactionKind::ProgrammableTransaction)
}

pub fn transaction_data_gen(sender: SuiAddress) -> impl Strategy<Value = TransactionData> {
    TransactionDataGenBuilder::new(sender)
        .kind(gen_transaction_kind())
        .gas_data(gen_gas_data(sender))
        .expiration(gen_transaction_expiration())
        .finish()
}

pub struct TransactionDataGenBuilder<
    K: Strategy<Value = TransactionKind>,
    G: Strategy<Value = GasData>,
    E: Strategy<Value = TransactionExpiration>,
> {
    pub kind: Option<K>,
    pub sender: SuiAddress,
    pub gas_data: Option<G>,
    pub expiration: Option<E>,
}

impl<
        K: Strategy<Value = TransactionKind>,
        G: Strategy<Value = GasData>,
        E: Strategy<Value = TransactionExpiration>,
    > TransactionDataGenBuilder<K, G, E>
{
    pub fn new(sender: SuiAddress) -> Self {
        Self {
            kind: None,
            sender,
            gas_data: None,
            expiration: None,
        }
    }

    pub fn kind(mut self, kind: K) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn gas_data(mut self, gas_data: G) -> Self {
        self.gas_data = Some(gas_data);
        self
    }

    pub fn expiration(mut self, expiration: E) -> Self {
        self.expiration = Some(expiration);
        self
    }

    pub fn finish(self) -> impl Strategy<Value = TransactionData> {
        (
            self.kind.expect("kind must be set"),
            Just(self.sender),
            self.gas_data.expect("gas_data must be set"),
            self.expiration.expect("expiration must be set"),
        )
            .prop_map(|(kind, sender, gas_data, expiration)| TransactionDataV1 {
                kind,
                sender,
                gas_data,
                expiration,
            })
            .prop_map(TransactionData::V1)
    }
}
