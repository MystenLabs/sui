// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::ObjectFundsAvailable;
use move_core_types::u256::U256;

#[test]
fn store_read_is_needed_only_when_uncached_balance_is_insufficient() {
    let empty = ObjectFundsAvailable::init();
    assert!(!empty.needs_store_read(U256::from(0u64)));
    assert!(empty.needs_store_read(U256::from(1u64)));

    let deposited = ObjectFundsAvailable {
        available: U256::from(500u64),
        queried: false,
    };
    assert!(!deposited.needs_store_read(U256::from(500u64)));
    assert!(deposited.needs_store_read(U256::from(501u64)));

    let queried = ObjectFundsAvailable {
        available: U256::from(0u64),
        queried: true,
    };
    assert!(!queried.needs_store_read(U256::from(1u64)));
}

mod balance_tracking {
    use crate::object_runtime::{
        BalanceTotals, BalancesBefore, TrackedContents, add_to_totals, layout_contains_balance,
        object_contents, walk_contents,
    };
    use move_core_types::{
        annotated_value::{MoveEnumLayout, MoveFieldLayout, MoveStructLayout, MoveTypeLayout},
        identifier::Identifier,
        language_storage::StructTag,
    };
    use std::{collections::BTreeSet, convert::Infallible, rc::Rc};
    use sui_types::{
        MOVE_STDLIB_ADDRESS, TypeTag,
        balance::Balance,
        base_types::{ObjectID, SequenceNumber, SuiAddress},
        gas_coin::GAS,
        id::UID,
        object::MoveObject,
    };

    fn coin_type(name: &str) -> TypeTag {
        TypeTag::Struct(Box::new(StructTag {
            address: ObjectID::from_single_byte(0xc0).into(),
            module: Identifier::new("coin_mod").unwrap(),
            name: Identifier::new(name).unwrap(),
            type_params: vec![],
        }))
    }

    fn struct_layout(name: &str, fields: Vec<(&str, MoveTypeLayout)>) -> MoveTypeLayout {
        MoveTypeLayout::Struct(Box::new(MoveStructLayout {
            type_: StructTag {
                address: ObjectID::from_single_byte(0xaa).into(),
                module: Identifier::new("m").unwrap(),
                name: Identifier::new(name).unwrap(),
                type_params: vec![],
            },
            fields: fields
                .into_iter()
                .map(|(n, l)| MoveFieldLayout::new(Identifier::new(n).unwrap(), l))
                .collect(),
        }))
    }

    fn uid() -> MoveTypeLayout {
        MoveTypeLayout::Struct(Box::new(UID::layout()))
    }

    fn balance(t: &TypeTag) -> MoveTypeLayout {
        MoveTypeLayout::Struct(Box::new(Balance::layout(t.clone())))
    }

    fn option(inner: MoveTypeLayout) -> MoveTypeLayout {
        MoveTypeLayout::Struct(Box::new(MoveStructLayout {
            type_: StructTag {
                address: MOVE_STDLIB_ADDRESS,
                module: Identifier::new("option").unwrap(),
                name: Identifier::new("Option").unwrap(),
                type_params: vec![],
            },
            fields: vec![MoveFieldLayout::new(
                Identifier::new("vec").unwrap(),
                MoveTypeLayout::Vector(Box::new(inner)),
            )],
        }))
    }

    fn totals(entries: &[(ObjectID, &TypeTag, u128)]) -> BalanceTotals {
        let mut totals = BalanceTotals::new();
        for (object, type_, value) in entries {
            let type_ = Rc::new((*type_).clone());
            add_to_totals(&mut totals, (*object).into(), &type_, *value).unwrap();
        }
        totals
    }

    fn contents_of(obj: &MoveObject, layout: &MoveTypeLayout) -> TrackedContents {
        let Ok(contents) = object_contents(obj, true, || Ok::<_, Infallible>(Some(layout)));
        contents.unwrap()
    }

    #[test]
    fn balance_in_wrapped_object_counts_toward_the_wrapped_object() {
        let a = coin_type("A");
        let outer_id = ObjectID::from_single_byte(1);
        let inner_id = ObjectID::from_single_byte(2);
        let inner = struct_layout("Inner", vec![("id", uid()), ("b", balance(&a))]);
        // A struct without a UID does not start a new object.
        let plain = struct_layout("Plain", vec![("b", balance(&a))]);
        let layout = struct_layout(
            "Outer",
            vec![
                ("id", uid()),
                ("before", balance(&a)),
                ("inner", inner),
                ("plain", plain),
                ("after", balance(&a)),
            ],
        );
        let bytes = bcs::to_bytes(&(outer_id, 10u64, (inner_id, 20u64), 30u64, 40u64)).unwrap();
        let contents = walk_contents(&layout, &bytes, true).unwrap();
        assert_eq!(
            contents,
            TrackedContents {
                uids: BTreeSet::from([outer_id, inner_id]),
                balances: totals(&[(outer_id, &a, 80), (inner_id, &a, 20)]),
            }
        );

        let untracked = walk_contents(&layout, &bytes, false).unwrap();
        assert_eq!(untracked.uids, contents.uids);
        assert!(untracked.balances.is_empty());
    }

    #[test]
    fn balances_in_vectors_and_options() {
        let a = coin_type("A");
        let b = coin_type("B");
        let id = ObjectID::from_single_byte(1);
        let layout = struct_layout(
            "Holder",
            vec![
                ("id", uid()),
                ("v", MoveTypeLayout::Vector(Box::new(balance(&a)))),
                ("some", option(balance(&b))),
                ("none", option(balance(&b))),
            ],
        );
        let bytes = bcs::to_bytes(&(id, vec![1u64, 2, 3], vec![7u64], Vec::<u64>::new())).unwrap();
        let contents = walk_contents(&layout, &bytes, true).unwrap();
        assert_eq!(contents.balances, totals(&[(id, &a, 6), (id, &b, 7)]));
    }

    fn enum_layout(variants: Vec<Vec<(&str, MoveTypeLayout)>>) -> MoveTypeLayout {
        MoveTypeLayout::Enum(Box::new(MoveEnumLayout {
            type_: StructTag {
                address: ObjectID::from_single_byte(0xaa).into(),
                module: Identifier::new("m").unwrap(),
                name: Identifier::new("E").unwrap(),
                type_params: vec![],
            },
            variants: variants
                .into_iter()
                .enumerate()
                .map(|(tag, fields)| {
                    let name = Identifier::new(format!("V{tag}")).unwrap();
                    let fields = fields
                        .into_iter()
                        .map(|(n, l)| MoveFieldLayout::new(Identifier::new(n).unwrap(), l))
                        .collect();
                    ((name, tag as u16), fields)
                })
                .collect(),
        }))
    }

    #[test]
    fn balances_in_enum_variants() {
        let a = coin_type("A");
        let id = ObjectID::from_single_byte(1);
        let e = enum_layout(vec![vec![("b", balance(&a))], vec![]]);
        let layout = struct_layout("Holder", vec![("id", uid()), ("e", e.clone())]);
        assert!(layout_contains_balance(&e));
        assert!(!layout_contains_balance(&enum_layout(vec![
            vec![("x", MoveTypeLayout::U64)],
            vec![]
        ])));

        let some = bcs::to_bytes(&(id, 0u8, 9u64)).unwrap();
        let contents = walk_contents(&layout, &some, true).unwrap();
        assert_eq!(contents.balances, totals(&[(id, &a, 9)]));

        let none = bcs::to_bytes(&(id, 1u8)).unwrap();
        let contents = walk_contents(&layout, &none, true).unwrap();
        assert!(contents.balances.is_empty());
        assert_eq!(contents.uids, BTreeSet::from([id]));
    }

    #[test]
    fn layout_without_balance_is_pruned() {
        let a = coin_type("A");
        let none = struct_layout(
            "NoBalance",
            vec![
                ("id", uid()),
                ("v", MoveTypeLayout::Vector(Box::new(uid()))),
            ],
        );
        assert!(!layout_contains_balance(&none));
        let nested = struct_layout("Nested", vec![("id", uid()), ("o", option(balance(&a)))]);
        assert!(layout_contains_balance(&nested));
    }

    #[test]
    fn coin_shortcut_matches_layout_walk() {
        let a = coin_type("A");
        let id = ObjectID::from_single_byte(3);
        let coin = MoveObject::new_coin(a.clone(), SequenceNumber::new(), id, 42);
        let layout = struct_layout("Coin", vec![("id", uid()), ("balance", balance(&a))]);
        let walked = walk_contents(&layout, coin.contents(), true).unwrap();
        let shortcut = contents_of(&coin, &layout);
        assert_eq!(shortcut, walked);
        assert_eq!(shortcut.balances, totals(&[(id, &a, 42)]));

        let gas = MoveObject::new_gas_coin(SequenceNumber::new(), id, 5);
        assert_eq!(
            contents_of(&gas, &layout).balances,
            totals(&[(id, &GAS::type_tag(), 5)])
        );
    }

    #[test]
    fn totals_sum_past_u64_into_u128() {
        let a = coin_type("A");
        let b = coin_type("B");
        let x = ObjectID::from_single_byte(1);
        let y = ObjectID::from_single_byte(2);
        let mut before = BalancesBefore::new();
        before.record(
            x,
            TrackedContents {
                uids: BTreeSet::from([x, y]),
                balances: totals(&[
                    (x, &a, u64::MAX as u128),
                    (x, &a, u64::MAX as u128),
                    (y, &a, 1),
                    (x, &b, 2),
                ]),
            },
        );
        let (totals, uids) = before.finish().unwrap();
        assert_eq!(uids, BTreeSet::from([x, y]));
        assert_eq!(totals[&SuiAddress::from(x)][&a], 2 * (u64::MAX as u128));
        assert_eq!(totals[&SuiAddress::from(y)][&a], 1);
        assert_eq!(totals[&SuiAddress::from(x)][&b], 2);
    }

    #[test]
    fn repeated_read_of_an_object_replaces_it() {
        let a = coin_type("A");
        let x = ObjectID::from_single_byte(1);
        let contents = TrackedContents {
            uids: BTreeSet::from([x]),
            balances: totals(&[(x, &a, 5)]),
        };
        let mut before = BalancesBefore::new();
        before.record(x, contents.clone());
        before.record(x, contents);
        let (totals, _) = before.finish().unwrap();
        assert_eq!(totals[&SuiAddress::from(x)][&a], 5);
    }

    #[test]
    fn total_overflow_is_an_error() {
        let a = Rc::new(coin_type("A"));
        let owner = SuiAddress::from(ObjectID::from_single_byte(1));
        let mut totals = BalanceTotals::new();
        add_to_totals(&mut totals, owner, &a, u128::MAX - 1).unwrap();
        add_to_totals(&mut totals, owner, &a, 1).unwrap();
        assert!(add_to_totals(&mut totals, owner, &a, 1).is_err());
        assert_eq!(totals[&owner][&a], u128::MAX);
    }
}
