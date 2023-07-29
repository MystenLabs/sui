// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_symbol_pool::Symbol;

use crate::diagnostics::codes::{custom, DiagnosticInfo, Severity};

pub mod id_leak;
pub mod typing;

pub const INIT_FUNCTION_NAME: Symbol = symbol!("init");

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

pub const SUI_SYSTEM_ADDR_NAME: Symbol = symbol!("sui_system");
pub const SUI_SYSTEM_MODULE_NAME: Symbol = symbol!("sui_system");
pub const SUI_SYSTEM_CREATE: Symbol = symbol!("create");
pub const CLOCK_MODULE_NAME: Symbol = symbol!("clock");
pub const CLOCK_TYPE_NAME: Symbol = symbol!("Clock");
pub const SUI_CLOCK_CREATE: Symbol = symbol!("create");

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

pub const SCRIPT_DIAG: DiagnosticInfo = custom(
    SUI_DIAG_PREFIX,
    Severity::NonblockingError,
    /* category */ TYPING,
    /* code */ 1,
    "scripts are not supported",
);
pub const ENTRY_FUN_SIGNATURE_DIAG: DiagnosticInfo = custom(
    SUI_DIAG_PREFIX,
    Severity::NonblockingError,
    /* category */ TYPING,
    /* code */ 2,
    "invalid 'entry' function signature",
);
