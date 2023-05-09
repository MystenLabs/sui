// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::language_storage::StructTag;
use serde::Serialize;
use serde_json::Value;
use serde_with::serde_as;
use sui_types::parse_sui_struct_tag;
use sui_types::sui_serde::SuiStructTag;

#[test]
fn test_struct_tag_serde() {
    let tag = parse_sui_struct_tag("0x7f89cdffd8968affa0b47bef91adc5314e19509080470c45bfd434cd83a766b::suifrens::SuiFren<0x7f89cdffd8968affa0b47bef91adc5314e19509080470c45bfd434cd83a766b::capy::Capy>").unwrap();
    #[serde_as]
    #[derive(Serialize)]
    struct TestStructTag(#[serde_as(as = "SuiStructTag")] StructTag);

    // serialize to json should not trim the leading 0
    let Value::String(json) = serde_json::to_value(&TestStructTag(tag.clone()))
        .unwrap() else { panic!() };
    assert_eq!(json, "0x07f89cdffd8968affa0b47bef91adc5314e19509080470c45bfd434cd83a766b::suifrens::SuiFren<0x07f89cdffd8968affa0b47bef91adc5314e19509080470c45bfd434cd83a766b::capy::Capy>");

    let tag2 = parse_sui_struct_tag(&json).unwrap();
    assert_eq!(tag, tag2);
}
