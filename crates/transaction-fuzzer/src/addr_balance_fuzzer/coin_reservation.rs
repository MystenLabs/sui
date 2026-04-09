// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use move_core_types::language_storage::TypeTag;
use proptest::prelude::*;

use sui_types::accumulator_root::AccumulatorValue;
use sui_types::balance::Balance;
use sui_types::base_types::{EpochId, ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::coin_reservation::ParsedObjectRefWithdrawal;
use sui_types::digests::ChainIdentifier;

use super::common::{TxFuzzContext, boundary_u64};

fn accumulator_object_id(sender: SuiAddress, fund_type: &TypeTag) -> ObjectID {
    *AccumulatorValue::get_field_id(sender, &Balance::type_tag(fund_type.clone()))
        .unwrap()
        .inner()
}

pub(super) fn coin_reservation_ref(
    sender: SuiAddress,
    fund_type: &TypeTag,
    epoch: EpochId,
    amount: u64,
    chain: ChainIdentifier,
) -> ObjectRef {
    let acc_id = accumulator_object_id(sender, fund_type);
    // Cap epoch to u32 because ParsedDigest stores epoch as u32; out-of-range
    // values would panic during construction (test data only — not a real bug).
    let epoch = epoch.min(u32::MAX as u64);
    ParsedObjectRefWithdrawal::new(acc_id, epoch, amount).encode(SequenceNumber::new(), chain)
}

/// Generates coin reservation ObjectRefs against `ctx.fund_type`. Mostly produces
/// well-formed reservations for the current sender/epoch/chain, with boundary arms
/// probing each field independently (zero amount, wrong epoch, wrong chain, wrong sender).
pub(super) fn coin_reservation_strategy(ctx: &TxFuzzContext) -> BoxedStrategy<ObjectRef> {
    coin_reservation_strategy_for(ctx.sender, ctx.fund_type.clone(), ctx.epoch, ctx.chain)
}

/// Generates a *valid* coin reservation: correct sender/epoch/chain and a small
/// amount well under the funded per-sender balance. Used by valid PT blocks that
/// need a reservation that can actually pass validation and execution.
pub(super) fn valid_coin_reservation_strategy(ctx: &TxFuzzContext) -> BoxedStrategy<ObjectRef> {
    let sender = ctx.sender;
    let fund_type = ctx.fund_type.clone();
    let epoch = ctx.epoch;
    let chain = ctx.chain;
    (1u64..=100u64)
        .prop_map(move |amount| coin_reservation_ref(sender, &fund_type, epoch, amount, chain))
        .boxed()
}

/// Same as `coin_reservation_strategy` but for an explicit fund type — used so the
/// gas-payment strategy can produce SUI reservations while PT inputs use the
/// custom `ctx.fund_type`.
pub(super) fn coin_reservation_strategy_for(
    sender: SuiAddress,
    fund_type: Arc<TypeTag>,
    epoch: EpochId,
    chain: ChainIdentifier,
) -> BoxedStrategy<ObjectRef> {
    prop_oneof![
        6 => boundary_u64().prop_map({
            let ft = fund_type.clone();
            move |amount| coin_reservation_ref(sender, &ft, epoch, amount.max(1), chain)
        }),
        1 => Just(coin_reservation_ref(sender, &fund_type, epoch, 0, chain)),
        1 => boundary_u64().prop_map({
            let ft = fund_type.clone();
            move |e| coin_reservation_ref(sender, &ft, e, 1, chain)
        }),
        1 => (any::<ChainIdentifier>(), boundary_u64()).prop_map({
            let ft = fund_type.clone();
            move |(c, amount)| coin_reservation_ref(sender, &ft, epoch, amount.max(1), c)
        }),
        1 => (any::<SuiAddress>(), boundary_u64()).prop_map({
            let ft = fund_type;
            move |(random_sender, amount)| {
                coin_reservation_ref(random_sender, &ft, epoch, amount.max(1), chain)
            }
        }),
    ]
    .boxed()
}
