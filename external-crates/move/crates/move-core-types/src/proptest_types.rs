// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    account_address::AccountAddress,
    annotated_value as A,
    identifier::Identifier,
    language_storage::{StructTag, TypeTag},
    runtime_value as R,
};
use proptest::{collection::vec, prelude::*};
use std::collections::BTreeMap;

impl Arbitrary for TypeTag {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        use TypeTag::*;
        let leaf = prop_oneof![
            Just(Bool),
            Just(U8),
            Just(U16),
            Just(U32),
            Just(U64),
            Just(U128),
            Just(U256),
            Just(Address),
            Just(Vector(Box::new(Bool))),
        ];
        leaf.prop_recursive(
            8,  // levels deep
            16, // max size
            4,  // max number of items per collection
            |inner| {
                (
                    any::<AccountAddress>(),
                    any::<Identifier>(),
                    any::<Identifier>(),
                    vec(inner, 0..4),
                )
                    .prop_map(|(address, module, name, type_params)| {
                        Struct(Box::new(StructTag {
                            address,
                            module,
                            name,
                            type_params,
                        }))
                    })
            },
        )
        .boxed()
    }
}

// =============================================================================
// Layout strategies
// =============================================================================

/// Strategy producing arbitrary runtime tree-form `MoveTypeLayout`s. Bounded
/// in depth and breadth so generated layouts stay reasonable for property tests.
pub fn arb_runtime_layout() -> BoxedStrategy<R::MoveTypeLayout> {
    use R::MoveTypeLayout as T;

    let leaf = prop_oneof![
        Just(T::Bool),
        Just(T::U8),
        Just(T::U16),
        Just(T::U32),
        Just(T::U64),
        Just(T::U128),
        Just(T::U256),
        Just(T::Address),
        Just(T::Signer),
    ];

    leaf.prop_recursive(
        4,  // depth
        24, // max nodes
        4,  // branching
        |inner| {
            prop_oneof![
                inner.clone().prop_map(|t| T::Vector(Box::new(t))),
                vec(inner.clone(), 0..=4)
                    .prop_map(|fields| T::Struct(Box::new(R::MoveStructLayout(Box::new(fields))))),
                vec(vec(inner, 0..=3), 1..=3)
                    .prop_map(|variants| T::Enum(Box::new(R::MoveEnumLayout(Box::new(variants))))),
            ]
        },
    )
    .boxed()
}

/// Strategy producing arbitrary annotated tree-form `MoveTypeLayout`s, including
/// type tags and field/variant names. Bounded in depth and breadth.
pub fn arb_annotated_layout() -> BoxedStrategy<A::MoveTypeLayout> {
    use A::MoveTypeLayout as T;

    let leaf = prop_oneof![
        Just(T::Bool),
        Just(T::U8),
        Just(T::U16),
        Just(T::U32),
        Just(T::U64),
        Just(T::U128),
        Just(T::U256),
        Just(T::Address),
        Just(T::Signer),
    ];

    leaf.prop_recursive(
        4,  // depth
        24, // max nodes
        4,  // branching
        |inner| {
            let struct_tag = (
                any::<AccountAddress>(),
                any::<Identifier>(),
                any::<Identifier>(),
            )
                .prop_map(|(address, module, name)| StructTag {
                    address,
                    module,
                    name,
                    type_params: vec![],
                });

            let field = (any::<Identifier>(), inner.clone())
                .prop_map(|(name, layout)| A::MoveFieldLayout { name, layout });

            let variant_fields = vec(field.clone(), 0..=3);

            // Build distinct (variant_name, tag) keys for the map.
            let variants = vec((any::<Identifier>(), variant_fields.clone()), 1..=3).prop_map(
                |entries| -> BTreeMap<(Identifier, u16), Vec<A::MoveFieldLayout>> {
                    let mut map = BTreeMap::new();
                    let mut seen: std::collections::HashSet<Identifier> =
                        std::collections::HashSet::new();
                    for (tag, (mut name, fields)) in entries.into_iter().enumerate() {
                        let tag = tag as u16;
                        // Ensure unique variant name within the enum.
                        while !seen.insert(name.clone()) {
                            name = Identifier::new(format!("{}_{}", name.as_str(), tag)).unwrap();
                        }
                        map.insert((name, tag), fields);
                    }
                    map
                },
            );

            prop_oneof![
                inner.clone().prop_map(|t| T::Vector(Box::new(t))),
                (struct_tag.clone(), vec(field, 0..=4)).prop_map(|(type_, fields)| T::Struct(
                    Box::new(A::MoveStructLayout { type_, fields })
                )),
                (struct_tag, variants).prop_map(|(type_, variants)| T::Enum(Box::new(
                    A::MoveEnumLayout { type_, variants }
                ))),
            ]
        },
    )
    .boxed()
}
