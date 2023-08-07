// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_enum_compat_util::*;

use crate::{SuiMoveStruct, SuiMoveValue};

#[test]
fn enforce_order_test() {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "staged", "sui_move_struct.yaml"]);
    check_enum_compat_order::<SuiMoveStruct>(path);

    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "staged", "sui_move_value.yaml"]);
    check_enum_compat_order::<SuiMoveValue>(path);
}
