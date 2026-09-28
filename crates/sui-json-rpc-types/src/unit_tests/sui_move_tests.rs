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

#[test]
fn signed_move_values_json_shape() {
    use move_core_types::annotated_value::MoveValue;
    use move_core_types::i256::I256;
    use serde_json::json;
    use std::str::FromStr;

    // Signed integers mirror the unsigned convention: i8/i16/i32 are native JSON numbers,
    // i64/i128/i256 are strings.
    let cases: &[(MoveValue, serde_json::Value)] = &[
        (MoveValue::I8(-1), json!(-1)),
        (MoveValue::I8(i8::MIN), json!(-128)),
        (MoveValue::I16(-424), json!(-424)),
        (MoveValue::I32(-432_432), json!(-432_432)),
        (MoveValue::I64(-432_432_432_432), json!("-432432432432")),
        (
            MoveValue::I128(-424_242_424_242_424_242_424),
            json!("-424242424242424242424"),
        ),
        (
            MoveValue::I256(I256::from_str("-42424242424242424242424242424242424242424").unwrap()),
            json!("-42424242424242424242424242424242424242424"),
        ),
    ];
    for (value, expect) in cases {
        let sui_value = SuiMoveValue::from(value.clone());
        assert_eq!(
            &serde_json::to_value(&sui_value).unwrap(),
            expect,
            "serde shape for {value:?}"
        );
        assert_eq!(
            &sui_value.to_json_value(),
            expect,
            "json shape for {value:?}"
        );
    }
}
