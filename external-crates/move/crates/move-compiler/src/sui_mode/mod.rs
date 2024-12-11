// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_symbol_pool::Symbol;

use crate::diagnostics::codes::{custom, DiagnosticInfo, Severity};

pub mod id_leak;
pub mod info;
pub mod linters;
pub mod typing;

// DEEPBOOK_ADDRESS / DEEPBOOK_PACKAGE_ID = 0xdee9;

pub const STD_ADDR_VALUE: AccountAddress = AccountAddress::from_suffix(0x1);
pub const SUI_ADDR_VALUE: AccountAddress = AccountAddress::from_suffix(0x2);
pub const SUI_SYSTEM_ADDR_VALUE: AccountAddress = AccountAddress::from_suffix(0x3);
pub const BRIDGE_ADDR_VALUE: AccountAddress = AccountAddress::from_suffix(0xb);

pub const INIT_FUNCTION_NAME: Symbol = symbol!("init");
pub const ID_FIELD_NAME: Symbol = symbol!("id");

pub const STD_ADDR_NAME: Symbol = symbol!("std");
pub const OPTION_MODULE_NAME: Symbol = symbol!("option");
pub const OPTION_TYPE_NAME: Symbol = symbol!("Option");
pub const UTF_MODULE_NAME: Symbol = symbol!("string");
pub const UTF_TYPE_NAME: Symbol = symbol!("String");
pub const ASCII_MODULE_NAME: Symbol = symbol!("ascii");
pub const ASCII_TYPE_NAME: Symbol = symbol!("String");

pub const SUI_ADDR_NAME: Symbol = symbol!("sui");
pub const OBJECT_MODULE_NAME: Symbol = symbol!("object");
pub const OBJECT_NEW: Symbol = symbol!("new");
pub const OBJECT_NEW_UID_FROM_HASH: Symbol = symbol!("new_uid_from_hash");
pub const TEST_SCENARIO_MODULE_NAME: Symbol = symbol!("test_scenario");
pub const TS_NEW_OBJECT: Symbol = symbol!("new_object");
pub const UID_TYPE_NAME: Symbol = symbol!("UID");
pub const ID_TYPE_NAME: Symbol = symbol!("ID");
pub const TX_CONTEXT_MODULE_NAME: Symbol = symbol!("tx_context");
pub const TX_CONTEXT_TYPE_NAME: Symbol = symbol!("TxContext");
pub const SUI_MODULE_NAME: Symbol = symbol!("sui");
pub const SUI_OTW_NAME: Symbol = symbol!("SUI");

pub const SUI_SYSTEM_ADDR_NAME: Symbol = symbol!("sui_system");
pub const SUI_SYSTEM_MODULE_NAME: Symbol = symbol!("sui_system");
pub const SUI_SYSTEM_CREATE: Symbol = symbol!("create");
pub const CLOCK_MODULE_NAME: Symbol = symbol!("clock");
pub const CLOCK_TYPE_NAME: Symbol = symbol!("Clock");
pub const SUI_CLOCK_CREATE: Symbol = symbol!("create");
pub const AUTHENTICATOR_STATE_MODULE_NAME: Symbol = symbol!("authenticator_state");
pub const AUTHENTICATOR_STATE_TYPE_NAME: Symbol = symbol!("AuthenticatorState");
pub const AUTHENTICATOR_STATE_CREATE: Symbol = symbol!("create");
pub const RANDOMNESS_MODULE_NAME: Symbol = symbol!("random");
pub const RANDOMNESS_STATE_TYPE_NAME: Symbol = symbol!("Random");
pub const RANDOMNESS_STATE_CREATE: Symbol = symbol!("create");
pub const DENY_LIST_MODULE_NAME: Symbol = symbol!("deny_list");
pub const DENY_LIST_CREATE: Symbol = symbol!("create");
pub const BRIDGE_ADDR_NAME: Symbol = symbol!("bridge");
pub const BRIDGE_MODULE_NAME: Symbol = symbol!("bridge");
pub const BRIDGE_TYPE_NAME: Symbol = symbol!("Bridge");
pub const BRIDGE_CREATE: Symbol = symbol!("create");

pub const EVENT_MODULE_NAME: Symbol = symbol!("event");
pub const EVENT_FUNCTION_NAME: Symbol = symbol!("emit");

pub const TRANSFER_MODULE_NAME: Symbol = symbol!("transfer");
pub const TRANSFER_FUNCTION_NAME: Symbol = symbol!("transfer");
pub const FREEZE_FUNCTION_NAME: Symbol = symbol!("freeze_object");
pub const SHARE_FUNCTION_NAME: Symbol = symbol!("share_object");
pub const RECEIVE_FUNCTION_NAME: Symbol = symbol!("receive");
pub const RECEIVING_TYPE_NAME: Symbol = symbol!("Receiving");

pub const PRIVATE_TRANSFER_FUNCTIONS: &[Symbol] = &[
    TRANSFER_FUNCTION_NAME,
    FREEZE_FUNCTION_NAME,
    SHARE_FUNCTION_NAME,
    RECEIVE_FUNCTION_NAME,
];

//**************************************************************************************************
// Diagnostics
//**************************************************************************************************

pub const SUI_DIAG_PREFIX: &str = "Sui ";

// Categories
pub const ID_LEAK_CATEGORY: u8 = 1;
pub const TYPING: u8 = 2;

pub const ID_LEAK_DIAG: DiagnosticInfo = custom(
    SUI_DIAG_PREFIX,
    Severity::NonblockingError,
    /* category */ ID_LEAK_CATEGORY,
    /* code */ 1,
    "invalid object construction",
);

pub const ENTRY_FUN_SIGNATURE_DIAG: DiagnosticInfo = custom(
    SUI_DIAG_PREFIX,
    Severity::NonblockingError,
    /* category */ TYPING,
    /* code */ 2,
    "invalid 'entry' function signature",
);
pub const INIT_FUN_DIAG: DiagnosticInfo = custom(
    SUI_DIAG_PREFIX,
    Severity::NonblockingError,
    /* category */ TYPING,
    /* code */ 3,
    "invalid 'init' function",
);
pub const OTW_DECL_DIAG: DiagnosticInfo = custom(
    SUI_DIAG_PREFIX,
    Severity::NonblockingError,
    /* category */ TYPING,
    /* code */ 4,
    "invalid one-time witness declaration",
);
pub const OTW_USAGE_DIAG: DiagnosticInfo = custom(
    SUI_DIAG_PREFIX,
    Severity::NonblockingError,
    /* category */ TYPING,
    /* code */ 5,
    "invalid one-time witness usage",
);
pub const INIT_CALL_DIAG: DiagnosticInfo = custom(
    SUI_DIAG_PREFIX,
    Severity::NonblockingError,
    /* category */ TYPING,
    /* code */ 6,
    "invalid 'init' call",
);
pub const OBJECT_DECL_DIAG: DiagnosticInfo = custom(
    SUI_DIAG_PREFIX,
    Severity::NonblockingError,
    /* category */ TYPING,
    /* code */ 7,
    "invalid object declaration",
);
pub const EVENT_EMIT_CALL_DIAG: DiagnosticInfo = custom(
    SUI_DIAG_PREFIX,
    Severity::NonblockingError,
    /* category */ TYPING,
    /* code */ 8,
    "invalid event",
);
pub const PRIVATE_TRANSFER_CALL_DIAG: DiagnosticInfo = custom(
    SUI_DIAG_PREFIX,
    Severity::NonblockingError,
    /* category */ TYPING,
    /* code */ 9,
    "invalid private transfer call",
);

// Bridge supported asset
pub const BRIDGE_SUPPORTED_ASSET: &[&str] = &["btc", "eth", "usdc", "usdt"];
