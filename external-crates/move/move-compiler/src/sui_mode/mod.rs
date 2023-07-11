// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::diagnostics::codes::{custom, DiagnosticInfo, Severity};

pub mod id_leak;

const SUI_ADDR_NAME: &str = "sui";
const OBJECT_MODULE_NAME: &str = "object";
const OBJECT_NEW: &str = "new";
const OBJECT_NEW_UID_FROM_HASH: &str = "new_uid_from_hash";
const TEST_SCENARIO_MODULE_NAME: &str = "test_scenario";
const TS_NEW_OBJECT: &str = "new_object";
const UID_TYPE_NAME: &str = "UID";

const SUI_SYSTEM_ADDR_NAME: &str = "sui_system";
const SUI_SYSTEM_MODULE_NAME: &str = "sui_system";
const SUI_SYSTEM_CREATE: &str = "create";
const CLOCK_MODULE_NAME: &str = "clock";
const SUI_CLOCK_CREATE: &str = "create";

const FRESH_ID_FUNCTIONS: &[(&str, &str, &str)] = &[
    (SUI_ADDR_NAME, OBJECT_MODULE_NAME, OBJECT_NEW),
    (SUI_ADDR_NAME, OBJECT_MODULE_NAME, OBJECT_NEW_UID_FROM_HASH),
    (SUI_ADDR_NAME, TEST_SCENARIO_MODULE_NAME, TS_NEW_OBJECT),
];
const FUNCTIONS_TO_SKIP: &[(&str, &str, &str)] = &[
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
