// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    non_execution_tables::{MAX_NUM_EVENT_EMIT, MAX_TX_GAS},
    units_types::GasCost,
};

//
// Native function costs
//
// TODO: need to refactor native gas calculation so it is extensible. Currently we
// have hardcoded here the stdlib natives.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[repr(u8)]
pub enum SuiNativeCostIndex {
    EVENT_EMIT = 0,

    OBJECT_BYTES_TO_ADDR = 1,
    OBJECT_BORROW_UUID = 2,
    OBJECT_DELETE_IMPL = 3,

    TRANSFER_TRANSFER_INTERNAL = 4,
    TRANSFER_FREEZE_OBJECT = 5,
    TRANSFER_SHARE_OBJECT = 6,

    TX_CONTEXT_DERIVE_ID = 7,
    TX_CONTEXT_NEW_SIGNER_FROM_ADDR = 8,
}

// Native costs are currently flat
// TODO recalibrate wrt bytecode costs
pub fn _native_cost_schedule() -> Vec<GasCost> {
    use SuiNativeCostIndex as N;

    let mut native_table = vec![
        // This is artificially chosen to limit too many event emits
        // We will change this in https://github.com/MystenLabs/sui/issues/3341
        (
            N::EVENT_EMIT,
            GasCost::new(MAX_TX_GAS / MAX_NUM_EVENT_EMIT, 1),
        ),
        (N::OBJECT_BYTES_TO_ADDR, GasCost::new(30, 1)),
        (N::OBJECT_BORROW_UUID, GasCost::new(150, 1)),
        (N::OBJECT_DELETE_IMPL, GasCost::new(100, 1)),
        (N::TRANSFER_TRANSFER_INTERNAL, GasCost::new(80, 1)),
        (N::TRANSFER_FREEZE_OBJECT, GasCost::new(80, 1)),
        (N::TRANSFER_SHARE_OBJECT, GasCost::new(80, 1)),
        (N::TX_CONTEXT_DERIVE_ID, GasCost::new(110, 1)),
        (N::TX_CONTEXT_NEW_SIGNER_FROM_ADDR, GasCost::new(200, 1)),
    ];
    native_table.sort_by_key(|cost| cost.0 as u64);
    native_table
        .into_iter()
        .map(|(_, cost)| cost)
        .collect::<Vec<_>>()
}
