// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_status::ExecutionFailureStatus;
use sui_enum_compat_util::*;
#[test]
fn enforce_order_test() {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "staged", "exec_failure_status.yaml"]);
    check_enum_compat_order::<ExecutionFailureStatus>(path);
}
