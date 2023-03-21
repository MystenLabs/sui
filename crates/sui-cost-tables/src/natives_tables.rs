// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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
