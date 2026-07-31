// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `sui_types::allowance` hand-writes the layout of `sui::allowance::Allowance` so signing can
//! read a funder and spender out of one without a layout resolver. Nothing in the type system
//! ties that layout to the Move source, so check it against the layout the compiler derives from
//! the built framework: a renamed field, a reordered field, or a changed enum tag fails here
//! rather than at signing time.

use move_bytecode_utils::layout::TypeLayoutBuilder;
use move_core_types::annotated_value as A;
use move_core_types::language_storage::TypeTag;
use sui_framework::BuiltInFramework;
use sui_types::allowance::Allowance;
use sui_types::balance::Balance;
use sui_types::gas_coin::GAS;
use sui_types::in_memory_storage::InMemoryStorage;

#[test]
fn allowance_layout_matches_framework() {
    let store = InMemoryStorage::new(BuiltInFramework::genesis_objects().collect());

    // `T` is phantom so it only appears in the tag, but exercise a real one anyway.
    let funds_type = Balance::type_tag(GAS::type_tag());
    let tag = TypeTag::Struct(Box::new(Allowance::type_(funds_type.clone())));

    let expected = TypeLayoutBuilder::build_with_types(&tag, &store)
        .expect("framework should define sui::allowance::Allowance");
    let actual = A::MoveTypeLayout::Struct(Box::new(Allowance::layout(funds_type)));

    assert_eq!(expected, actual);
}
