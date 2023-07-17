// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_symbol_pool::Symbol;

use crate::diagnostics::codes::{custom, DiagnosticInfo, Severity};

pub mod id_leak;

const SUI_ADDR_NAME: Symbol = symbol!("sui");
const OBJECT_MODULE_NAME: Symbol = symbol!("object");
const OBJECT_NEW: Symbol = symbol!("new");
const OBJECT_NEW_UID_FROM_HASH: Symbol = symbol!("new_uid_from_hash");
const TEST_SCENARIO_MODULE_NAME: Symbol = symbol!("test_scenario");
const TS_NEW_OBJECT: Symbol = symbol!("new_object");
const UID_TYPE_NAME: Symbol = symbol!("UID");

const SUI_SYSTEM_ADDR_NAME: Symbol = symbol!("sui_system");
const SUI_SYSTEM_MODULE_NAME: Symbol = symbol!("sui_system");
const SUI_SYSTEM_CREATE: Symbol = symbol!("create");
const CLOCK_MODULE_NAME: Symbol = symbol!("clock");
const SUI_CLOCK_CREATE: Symbol = symbol!("create");

const FRESH_ID_FUNCTIONS: &[(Symbol, Symbol, Symbol)] = &[
    (SUI_ADDR_NAME, OBJECT_MODULE_NAME, OBJECT_NEW),
    (SUI_ADDR_NAME, OBJECT_MODULE_NAME, OBJECT_NEW_UID_FROM_HASH),
    (SUI_ADDR_NAME, TEST_SCENARIO_MODULE_NAME, TS_NEW_OBJECT),
];
const FUNCTIONS_TO_SKIP: &[(Symbol, Symbol, Symbol)] = &[
    (
        SUI_SYSTEM_ADDR_NAME,
        SUI_SYSTEM_MODULE_NAME,
        SUI_SYSTEM_CREATE,
    ),
    (SUI_ADDR_NAME, CLOCK_MODULE_NAME, SUI_CLOCK_CREATE),
];

const ID_LEAK_DIAG: DiagnosticInfo = custom(
    "Sui ",
    Severity::NonblockingError,
    /* category */ 1,
    /* code */ 1,
    "invalid object construction",
);
