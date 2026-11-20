// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests for the UID and `Balance<T>` traversals. The property tests generate random layouts and
//! matching values. Each generated value carries the results its traversal should produce, which
//! are computed while the value is generated.

use super::*;
use crate::object_runtime::merge_totals;
use move_core_types::{account_address::AccountAddress, annotated_value::MoveTypeLayout as L};
use proptest::{
    collection::vec,
    prelude::*,
    sample::{Index, select},
};
use sui_types::{SUI_FRAMEWORK_ADDRESS, id::ID};

const MAX_DEPTH: u32 = 6;
const MAX_VEC_LEN: usize = 4;
const MAX_FIELDS: usize = 4;
/// The largest type allowed, as set by `max_type_nodes`.
const MAX_TYPE_NODES: usize = 256;

fn uleb(bytes: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            bytes.push(byte);
            return;
        }
        bytes.push(byte | 0x80);
    }
}

fn type_nodes(t: &TypeTag) -> usize {
    match t {
        TypeTag::Vector(inner) => 1 + type_nodes(inner),
        TypeTag::Struct(s) => 1 + s.type_params.iter().map(type_nodes).sum::<usize>(),
        _ => 1,
    }
}

#[test]
fn merge_overflow_is_an_error() {
    let a = coin_type("A");
    let x = ObjectID::from_single_byte(1);
    let mut into = totals(&[(x, &a, u128::MAX)]);
    assert!(merge_totals(&mut into, &totals(&[(x, &a, 1)])).is_err());
}

#[test]
fn finish_overflow_is_an_error() {
    let a = coin_type("A");
    let x = ObjectID::from_single_byte(1);
    let mut before = BalancesBefore::new();
    for key in [2, 3] {
        let contents = TrackedContents {
            uids: BTreeSet::from([x]),
            balances: totals(&[(x, &a, u128::MAX)]),
        };
        before
            .record(ObjectID::from_single_byte(key), contents)
            .unwrap();
    }
    assert!(before.finish().is_err());
}

#[test]
fn invalid_vector_len_is_error() {
    let mut bytes = vec![];
    uleb(&mut bytes, 10);
    for elem in [MoveTypeLayout::U8, uid()] {
        let layout = MoveTypeLayout::Vector(Box::new(elem));
        assert!(walk_contents(&layout, &bytes, false).is_err());
        assert!(walk_contents(&layout, &bytes, true).is_err());
    }
}

#[test]
fn unknown_enum_tag_is_an_error() {
    let e = enum_layout(vec![vec![("x", MoveTypeLayout::U8)], vec![]]);
    // Tag 2 is not a variant. 0x80 0x01 is tag 128 as a two-byte ULEB128.
    for bytes in [vec![2u8, 0], vec![0x80, 0x01, 0]] {
        assert!(walk_contents(&e, &bytes, false).is_err());
        assert!(walk_contents(&e, &bytes, true).is_err());
    }
}

/// A valid object never holds the same UID twice, but a dev inspect value built from bytes can.
/// The UID is found once, and the balances of the objects sharing it are summed.
#[test]
fn duplicate_uids_collapse_and_their_balances_merge() {
    let a = coin_type("A");
    let outer = ObjectID::from_single_byte(1);
    let inner = ObjectID::from_single_byte(2);
    let inner_layout = struct_layout("Inner", vec![("id", uid()), ("b", balance(&a))]);
    let layout = struct_layout(
        "Outer",
        vec![
            ("id", uid()),
            ("x", inner_layout.clone()),
            ("y", inner_layout),
            ("v", MoveTypeLayout::Vector(Box::new(uid()))),
        ],
    );
    let bytes =
        bcs::to_bytes(&(outer, (inner, 5u64), (inner, u64::MAX), vec![inner, outer])).unwrap();
    let contents = walk_contents(&layout, &bytes, true).unwrap();
    assert_eq!(
        contents,
        TrackedContents {
            uids: BTreeSet::from([outer, inner]),
            balances: totals(&[(inner, &a, 5), (inner, &a, u64::MAX as u128)]),
        }
    );
    assert_eq!(
        walk_contents(&layout, &bytes, false).unwrap().uids,
        contents.uids
    );
}

/// Balance types as large as `max_type_nodes` allows are keyed exactly: two balances of the same
/// type are summed, and a type that differs only in its innermost argument is kept separate.
#[test]
fn largest_balance_types_merge_and_stay_separate() {
    let nested = |leaf: TypeTag| {
        (1..MAX_TYPE_NODES).fold(leaf, |t, _| {
            TypeTag::Struct(Box::new(StructTag {
                address: ObjectID::from_single_byte(0xaa).into(),
                module: Identifier::new("m").unwrap(),
                name: Identifier::new("Foo").unwrap(),
                type_params: vec![t],
            }))
        })
    };
    let x = nested(TypeTag::U8);
    let y = nested(TypeTag::U16);
    assert_eq!(type_nodes(&x), MAX_TYPE_NODES);
    let id = ObjectID::from_single_byte(1);
    let layout = struct_layout(
        "O",
        vec![
            ("id", uid()),
            ("a", balance(&x)),
            ("b", balance(&x)),
            ("c", balance(&y)),
        ],
    );
    let bytes = bcs::to_bytes(&(id, u64::MAX, 1u64, 5u64)).unwrap();
    let contents = walk_contents(&layout, &bytes, true).unwrap();
    assert_eq!(
        contents.balances,
        totals(&[(id, &x, u64::MAX as u128), (id, &x, 1), (id, &y, 5)])
    );
}

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

fn identifier() -> impl Strategy<Value = Identifier> {
    "[A-Za-z][A-Za-z0-9_]{0,15}".prop_map(|s| Identifier::new(s).unwrap())
}

fn address() -> impl Strategy<Value = AccountAddress> {
    prop_oneof![
        1 => Just(MOVE_STDLIB_ADDRESS),
        1 => Just(SUI_FRAMEWORK_ADDRESS),
        3 => any::<[u8; AccountAddress::LENGTH]>().prop_map(AccountAddress::new),
    ]
}

fn primitive_type() -> impl Strategy<Value = TypeTag> {
    prop_oneof![
        Just(TypeTag::Bool),
        Just(TypeTag::U8),
        Just(TypeTag::U16),
        Just(TypeTag::U32),
        Just(TypeTag::U64),
        Just(TypeTag::U128),
        Just(TypeTag::U256),
        Just(TypeTag::Address),
    ]
}

/// A valid type argument for `Balance<T>`, which is any type except `signer`.
fn type_tag() -> BoxedStrategy<TypeTag> {
    primitive_type()
        .prop_recursive(4, 32, 3, |inner| {
            prop_oneof![
                inner.clone().prop_map(|t| TypeTag::Vector(Box::new(t))),
                (address(), identifier(), identifier(), vec(inner, 0..=3)).prop_map(
                    |(address, module, name, type_params)| {
                        TypeTag::Struct(Box::new(StructTag {
                            address,
                            module,
                            name,
                            type_params,
                        }))
                    }
                ),
            ]
        })
        .boxed()
}

/// A type with close to `MAX_TYPE_NODES` nodes, either deeply nested or with many type arguments.
fn large_type_tag() -> BoxedStrategy<TypeTag> {
    let deep = (
        MAX_TYPE_NODES / 2..=MAX_TYPE_NODES,
        address(),
        identifier(),
        identifier(),
        primitive_type(),
    )
        .prop_map(|(nodes, address, module, name, leaf)| {
            (1..nodes).fold(leaf, |t, i| {
                if i % 2 == 0 {
                    TypeTag::Vector(Box::new(t))
                } else {
                    TypeTag::Struct(Box::new(StructTag {
                        address,
                        module: module.clone(),
                        name: name.clone(),
                        type_params: vec![t],
                    }))
                }
            })
        });
    // Each argument has at most 7 nodes, so the type has at most 1 + 32 * 7 nodes.
    let argument = (0..=6usize, primitive_type())
        .prop_map(|(depth, leaf)| (0..depth).fold(leaf, |t, _| TypeTag::Vector(Box::new(t))));
    let wide = (
        address(),
        identifier(),
        identifier(),
        vec(argument, 16..=32),
    )
        .prop_map(|(address, module, name, type_params)| {
            TypeTag::Struct(Box::new(StructTag {
                address,
                module,
                name,
                type_params,
            }))
        });
    prop_oneof![deep, wide].boxed()
}

/// Changes node `i` of `t`, counted in preorder: a struct is renamed if `rename`, and any other
/// node is replaced by `replacement`. Returns whether the node was found.
fn change_node(t: &mut TypeTag, i: &mut usize, replacement: &TypeTag, rename: bool) -> bool {
    if *i == 0 {
        match t {
            TypeTag::Struct(s) if rename => {
                s.name = Identifier::new(format!("{}X", s.name)).unwrap();
            }
            _ => *t = replacement.clone(),
        }
        return true;
    }
    *i -= 1;
    match t {
        TypeTag::Vector(inner) => change_node(inner, i, replacement, rename),
        TypeTag::Struct(s) => s
            .type_params
            .iter_mut()
            .any(|p| change_node(p, i, replacement, rename)),
        _ => false,
    }
}

/// `t` with one node changed, such as `Foo<X>` to `Foo<Y>`.
fn near_miss(t: TypeTag) -> BoxedStrategy<TypeTag> {
    let nodes = type_nodes(&t);
    (any::<Index>(), type_tag(), any::<bool>())
        .prop_map(move |(i, replacement, rename)| {
            let mut changed = t.clone();
            change_node(&mut changed, &mut i.index(nodes), &replacement, rename);
            if changed == t {
                changed = TypeTag::Vector(Box::new(changed));
            }
            changed
        })
        .boxed()
}

/// Random types to share among the balances of one case.
fn type_pool() -> impl Strategy<Value = Vec<TypeTag>> {
    vec(type_tag(), 1..=4)
}

/// A type for `Balance<T>`. Most come from a fixed set or from the case's pool, so balances often
/// share a type and are summed. Others differ from a pool type in one place, are fresh, or are as
/// large as allowed.
fn balance_type(pool: &[TypeTag]) -> BoxedStrategy<TypeTag> {
    let common = vec![GAS::type_tag(), coin_type("A"), coin_type("B")];
    prop_oneof![
        3 => select(common),
        3 => select(pool.to_vec()),
        2 => select(pool.to_vec()).prop_flat_map(near_miss),
        1 => type_tag(),
        1 => large_type_tag(),
    ]
    .boxed()
}

fn is_framework_struct(s: &StructTag, module: &str, name: &str) -> bool {
    s.address == SUI_FRAMEWORK_ADDRESS && s.module.as_str() == module && s.name.as_str() == name
}

/// A random tag for a struct or enum, which is never a real `UID` or `Balance`.
fn container_tag() -> impl Strategy<Value = StructTag> {
    (
        address(),
        identifier(),
        identifier(),
        vec(type_tag(), 0..=2),
    )
        .prop_map(|(address, module, name, type_params)| StructTag {
            address,
            module,
            name,
            type_params,
        })
        .prop_filter("a real UID or Balance", |s| {
            !is_framework_struct(s, "object", "UID")
                && !is_framework_struct(s, "balance", "Balance")
        })
}

// -------------------------------------------------------------------------------------------------
// Values
// -------------------------------------------------------------------------------------------------

/// The type of a generated value. Every type takes at least one byte, as in Move, so a vector
/// length read from random bytes cannot make the traversal loop without reading.
#[derive(Clone, Debug)]
enum Shape {
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Bool,
    Address,
    Vector(Box<Shape>),
    /// `std::option::Option`, holding at most one element.
    Option(Box<Shape>),
    /// A struct that is not an object: it has at least one field, and the first is not a `UID`.
    Struct(StructTag, Vec<Field>),
    Enum(StructTag, Vec<Vec<Field>>),
    Uid,
    /// A struct whose first field, with this name, is a `UID`, followed by these fields.
    Object(StructTag, Identifier, Vec<Field>),
    Balance(TypeTag),
    /// A `UID` struct outside the framework, holding a real `ID`.
    FakeUid,
    /// An `ID` outside a `UID`.
    BareId,
    FakeBalance(FakeBalance),
}

#[derive(Clone, Debug)]
struct Field {
    name: Identifier,
    shape: Shape,
}

/// Structs that resemble `Balance<T>` but are not one.
#[derive(Clone, Debug)]
enum FakeBalance {
    WrongAddress(TypeTag),
    WrongFieldName(TypeTag),
    WrongFieldType(TypeTag),
    NoTypeParam,
    TwoTypeParams(TypeTag, TypeTag),
    /// `sui::balance::Supply<T>`, which has the same field as `Balance<T>`.
    Supply(TypeTag),
    /// A `value: u64` field followed by another field, which may hold real balances.
    ExtraField(TypeTag, Box<Shape>),
}

/// A generated value, with the results its traversal should produce.
#[derive(Clone, Debug, Default)]
struct Value {
    bytes: Vec<u8>,
    uids: BTreeSet<ObjectID>,
    /// Balances inside an object, as (innermost enclosing object, type, amount).
    attached: Vec<(ObjectID, TypeTag, u64)>,
    /// Balances not yet inside an object, as (type, amount).
    unattached: Vec<(TypeTag, u64)>,
}

impl Value {
    fn bytes(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            ..Self::default()
        }
    }

    /// `prefix` followed by each part, with their results combined.
    fn concat(prefix: Vec<u8>, parts: Vec<Value>) -> Self {
        let mut value = Self::bytes(prefix);
        for part in parts {
            value.bytes.extend(part.bytes);
            value.uids.extend(part.uids);
            value.attached.extend(part.attached);
            value.unattached.extend(part.unattached);
        }
        value
    }

    fn expected_totals(&self) -> BalanceTotals {
        let entries: Vec<_> = self
            .attached
            .iter()
            .map(|(id, t, amount)| (*id, t, u128::from(*amount)))
            .collect();
        totals(&entries)
    }
}

#[derive(Clone, Debug)]
struct Case {
    layout: MoveTypeLayout,
    value: Value,
    /// Whether the layout holds a real `Balance<T>` anywhere, even in an empty vector or an
    /// unused enum variant.
    contains_balance: bool,
}

/// Random IDs, so the UIDs in a value are distinct, as in a valid object, and lookalike IDs differ
/// from every real UID. Duplicate UIDs are covered by
/// `duplicate_uids_collapse_and_their_balances_merge`.
fn object_id() -> impl Strategy<Value = ObjectID> {
    any::<[u8; ObjectID::LENGTH]>().prop_map(ObjectID::new)
}

/// Amounts are biased toward the edges, so totals exceed `u64::MAX`.
fn amount() -> impl Strategy<Value = u64> {
    prop_oneof![
        Just(0u64),
        Just(1),
        Just(u64::MAX - 1),
        Just(u64::MAX),
        any::<u64>(),
    ]
}

fn fields(inner: impl Strategy<Value = Shape>, min: usize) -> impl Strategy<Value = Vec<Field>> {
    vec(
        (identifier(), inner).prop_map(|(name, shape)| Field { name, shape }),
        min..=MAX_FIELDS,
    )
}

fn shape(pool: Vec<TypeTag>) -> BoxedStrategy<Shape> {
    let fake_balance = prop_oneof![
        balance_type(&pool).prop_map(FakeBalance::WrongAddress),
        balance_type(&pool).prop_map(FakeBalance::WrongFieldName),
        balance_type(&pool).prop_map(FakeBalance::WrongFieldType),
        Just(FakeBalance::NoTypeParam),
        (balance_type(&pool), balance_type(&pool))
            .prop_map(|(a, b)| FakeBalance::TwoTypeParams(a, b)),
        balance_type(&pool).prop_map(FakeBalance::Supply),
    ];
    let leaf = prop_oneof![
        1 => Just(Shape::U8),
        1 => Just(Shape::U16),
        1 => Just(Shape::U32),
        1 => Just(Shape::U64),
        1 => Just(Shape::U128),
        1 => Just(Shape::U256),
        1 => Just(Shape::Bool),
        1 => Just(Shape::Address),
        3 => Just(Shape::Uid),
        1 => Just(Shape::FakeUid),
        1 => Just(Shape::BareId),
        4 => balance_type(&pool).prop_map(Shape::Balance),
        2 => fake_balance.prop_map(Shape::FakeBalance),
    ];
    leaf.prop_recursive(MAX_DEPTH, 64, MAX_FIELDS as u32, move |inner| {
        prop_oneof![
            1 => inner.clone().prop_map(|s| Shape::Vector(Box::new(s))),
            1 => inner.clone().prop_map(|s| Shape::Option(Box::new(s))),
            2 => (container_tag(), fields(inner.clone(), 1)).prop_map(plain_struct),
            1 => (container_tag(), vec(fields(inner.clone(), 0), 1..=MAX_FIELDS))
                .prop_map(|(tag, variants)| Shape::Enum(tag, variants)),
            3 => object_shape(inner.clone()),
            1 => (balance_type(&pool), inner).prop_map(|(t, s)| {
                Shape::FakeBalance(FakeBalance::ExtraField(t, Box::new(s)))
            }),
        ]
    })
    .boxed()
}

fn object_shape(inner: impl Strategy<Value = Shape>) -> impl Strategy<Value = Shape> {
    (container_tag(), identifier(), fields(inner, 0))
        .prop_map(|(tag, id, fields)| Shape::Object(tag, id, fields))
}

/// A struct whose first field is a `UID` is an object.
fn plain_struct((tag, mut fields): (StructTag, Vec<Field>)) -> Shape {
    if matches!(
        fields.first(),
        Some(Field {
            shape: Shape::Uid,
            ..
        })
    ) {
        let id = fields.remove(0).name;
        Shape::Object(tag, id, fields)
    } else {
        Shape::Struct(tag, fields)
    }
}

fn field_layouts(fields: &[Field]) -> Vec<MoveFieldLayout> {
    fields
        .iter()
        .map(|f| MoveFieldLayout::new(f.name.clone(), layout(&f.shape)))
        .collect()
}

fn layout(shape: &Shape) -> MoveTypeLayout {
    match shape {
        Shape::U8 => L::U8,
        Shape::U16 => L::U16,
        Shape::U32 => L::U32,
        Shape::U64 => L::U64,
        Shape::U128 => L::U128,
        Shape::U256 => L::U256,
        Shape::Bool => L::Bool,
        Shape::Address => L::Address,
        Shape::Vector(s) => L::Vector(Box::new(layout(s))),
        Shape::Option(s) => option(layout(s)),
        Shape::Struct(tag, fields) => L::Struct(Box::new(MoveStructLayout {
            type_: tag.clone(),
            fields: field_layouts(fields),
        })),
        Shape::Enum(tag, variants) => L::Enum(Box::new(MoveEnumLayout {
            type_: tag.clone(),
            variants: variants
                .iter()
                .enumerate()
                .map(|(i, fields)| {
                    let name = Identifier::new(format!("V{i}")).unwrap();
                    ((name, i as u16), field_layouts(fields))
                })
                .collect(),
        })),
        Shape::Uid => uid(),
        Shape::Object(tag, id, fields) => {
            let mut all = vec![MoveFieldLayout::new(id.clone(), uid())];
            all.extend(field_layouts(fields));
            L::Struct(Box::new(MoveStructLayout {
                type_: tag.clone(),
                fields: all,
            }))
        }
        Shape::Balance(t) => balance(t),
        Shape::FakeUid => L::Struct(Box::new(MoveStructLayout {
            type_: StructTag {
                address: ObjectID::from_single_byte(0xaa).into(),
                module: Identifier::new("object").unwrap(),
                name: Identifier::new("UID").unwrap(),
                type_params: vec![],
            },
            fields: vec![MoveFieldLayout::new(
                Identifier::new("id").unwrap(),
                L::Struct(Box::new(ID::layout())),
            )],
        })),
        Shape::BareId => L::Struct(Box::new(ID::layout())),
        Shape::FakeBalance(fake) => fake_balance_layout(fake),
    }
}

fn fake_balance_layout(fake: &FakeBalance) -> MoveTypeLayout {
    let framework = SUI_FRAMEWORK_ADDRESS;
    let (address, name, type_params, fields) = match fake {
        FakeBalance::WrongAddress(t) => (
            ObjectID::from_single_byte(0xaa).into(),
            "Balance",
            vec![t.clone()],
            vec![("value", L::U64)],
        ),
        FakeBalance::WrongFieldName(t) => (
            framework,
            "Balance",
            vec![t.clone()],
            vec![("amount", L::U64)],
        ),
        FakeBalance::WrongFieldType(t) => (
            framework,
            "Balance",
            vec![t.clone()],
            vec![("value", L::U128)],
        ),
        FakeBalance::NoTypeParam => (framework, "Balance", vec![], vec![("value", L::U64)]),
        FakeBalance::TwoTypeParams(a, b) => (
            framework,
            "Balance",
            vec![a.clone(), b.clone()],
            vec![("value", L::U64)],
        ),
        FakeBalance::Supply(t) => (
            framework,
            "Supply",
            vec![t.clone()],
            vec![("value", L::U64)],
        ),
        FakeBalance::ExtraField(t, s) => (
            framework,
            "Balance",
            vec![t.clone()],
            vec![("value", L::U64), ("extra", layout(s))],
        ),
    };
    L::Struct(Box::new(MoveStructLayout {
        type_: StructTag {
            address,
            module: Identifier::new("balance").unwrap(),
            name: Identifier::new(name).unwrap(),
            type_params,
        },
        fields: fields
            .into_iter()
            .map(|(n, l)| MoveFieldLayout::new(Identifier::new(n).unwrap(), l))
            .collect(),
    }))
}

fn contains_balance(shape: &Shape) -> bool {
    match shape {
        Shape::Balance(_) => true,
        Shape::Vector(s) | Shape::Option(s) | Shape::FakeBalance(FakeBalance::ExtraField(_, s)) => {
            contains_balance(s)
        }
        Shape::Struct(_, fields) | Shape::Object(_, _, fields) => {
            fields.iter().any(|f| contains_balance(&f.shape))
        }
        Shape::Enum(_, variants) => variants
            .iter()
            .flatten()
            .any(|f| contains_balance(&f.shape)),
        Shape::U8
        | Shape::U16
        | Shape::U32
        | Shape::U64
        | Shape::U128
        | Shape::U256
        | Shape::Bool
        | Shape::Address
        | Shape::Uid
        | Shape::FakeUid
        | Shape::BareId
        | Shape::FakeBalance(_) => false,
    }
}

fn fields_value(fields: &[Field]) -> BoxedStrategy<Value> {
    fields
        .iter()
        .map(|f| value(&f.shape))
        .collect::<Vec<_>>()
        .prop_map(|parts| Value::concat(vec![], parts))
        .boxed()
}

fn sequence(elem: &Shape, max_len: usize) -> BoxedStrategy<Value> {
    vec(value(elem), 0..=max_len)
        .prop_map(|elems| {
            let mut prefix = vec![];
            uleb(&mut prefix, elems.len() as u64);
            Value::concat(prefix, elems)
        })
        .boxed()
}

fn le_bytes<const N: usize>() -> BoxedStrategy<Value> {
    any::<[u8; N]>()
        .prop_map(|b| Value::bytes(b.to_vec()))
        .boxed()
}

fn amount_bytes() -> BoxedStrategy<Value> {
    amount()
        .prop_map(|a| Value::bytes(a.to_le_bytes().to_vec()))
        .boxed()
}

/// The ID in a lookalike, which is never found.
fn untracked_id_bytes() -> BoxedStrategy<Value> {
    object_id()
        .prop_map(|id| Value::bytes(id.into_bytes().to_vec()))
        .boxed()
}

fn value(shape: &Shape) -> BoxedStrategy<Value> {
    match shape {
        Shape::U8 => le_bytes::<1>(),
        Shape::U16 => le_bytes::<2>(),
        Shape::U32 => le_bytes::<4>(),
        Shape::U64 => le_bytes::<8>(),
        Shape::U128 => le_bytes::<16>(),
        Shape::U256 | Shape::Address => le_bytes::<32>(),
        Shape::Bool => any::<bool>()
            .prop_map(|b| Value::bytes(vec![b as u8]))
            .boxed(),
        Shape::Vector(s) => sequence(s, MAX_VEC_LEN),
        Shape::Option(s) => sequence(s, 1),
        Shape::Struct(_, fields) => fields_value(fields),
        Shape::Enum(_, variants) => {
            let variants = variants.clone();
            (0..variants.len())
                .prop_flat_map(move |tag| {
                    fields_value(&variants[tag])
                        .prop_map(move |v| Value::concat(vec![tag as u8], vec![v]))
                })
                .boxed()
        }
        Shape::Uid => object_id()
            .prop_map(|id| Value {
                bytes: id.into_bytes().to_vec(),
                uids: BTreeSet::from([id]),
                ..Value::default()
            })
            .boxed(),
        Shape::Object(_, _, fields) => (object_id(), fields_value(fields))
            .prop_map(|(id, fields)| {
                let mut value = Value::concat(id.into_bytes().to_vec(), vec![fields]);
                value.uids.insert(id);
                let unattached = std::mem::take(&mut value.unattached);
                value
                    .attached
                    .extend(unattached.into_iter().map(|(t, a)| (id, t, a)));
                value
            })
            .boxed(),
        Shape::Balance(t) => {
            let t = t.clone();
            amount()
                .prop_map(move |a| Value {
                    bytes: a.to_le_bytes().to_vec(),
                    unattached: vec![(t.clone(), a)],
                    ..Value::default()
                })
                .boxed()
        }
        Shape::FakeUid | Shape::BareId => untracked_id_bytes(),
        Shape::FakeBalance(FakeBalance::WrongFieldType(_)) => le_bytes::<16>(),
        Shape::FakeBalance(FakeBalance::ExtraField(_, s)) => (amount_bytes(), value(s))
            .prop_map(|(a, extra)| Value::concat(vec![], vec![a, extra]))
            .boxed(),
        Shape::FakeBalance(_) => amount_bytes(),
    }
}

fn case_of(shape: impl Strategy<Value = Shape> + 'static) -> BoxedStrategy<Case> {
    shape
        .prop_flat_map(|shape| {
            let layout = layout(&shape);
            let contains_balance = contains_balance(&shape);
            value(&shape).prop_map(move |value| Case {
                layout: layout.clone(),
                value,
                contains_balance,
            })
        })
        .boxed()
}

fn case() -> impl Strategy<Value = Case> {
    type_pool().prop_flat_map(|pool| case_of(shape(pool)))
}

/// Values read from storage are objects, so they are also generated on their own.
fn object_case(pool: Vec<TypeTag>) -> BoxedStrategy<Case> {
    case_of(object_shape(shape(pool)))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// The UIDs found are exactly the `UID`s in the value, with or without balance tracking.
    /// `UID` lookalikes and `ID`s outside a `UID` are not found.
    #[test]
    fn walk_finds_exactly_the_uids(case in case()) {
        let untracked = walk_contents(&case.layout, &case.value.bytes, false).unwrap();
        prop_assert_eq!(&untracked.uids, &case.value.uids);
        prop_assert!(untracked.balances.is_empty());
        if let Ok(tracked) = walk_contents(&case.layout, &case.value.bytes, true) {
            prop_assert_eq!(&tracked.uids, &case.value.uids);
        }
    }

    /// Each balance counts toward its innermost enclosing object, and a balance that is not inside
    /// any object is an error. `Balance` lookalikes are not counted.
    #[test]
    fn walk_credits_balances_to_the_innermost_object(case in case()) {
        let tracked = walk_contents(&case.layout, &case.value.bytes, true);
        if case.value.unattached.is_empty() {
            let tracked = tracked.unwrap();
            prop_assert_eq!(&tracked.uids, &case.value.uids);
            prop_assert_eq!(tracked.balances, case.value.expected_totals());
        } else {
            prop_assert!(tracked.is_err());
        }
    }

    #[test]
    fn walk_of_an_object(case in type_pool().prop_flat_map(object_case)) {
        let tracked = walk_contents(&case.layout, &case.value.bytes, true).unwrap();
        prop_assert_eq!(&tracked.uids, &case.value.uids);
        prop_assert_eq!(tracked.balances, case.value.expected_totals());
    }

    /// `layout_contains_balance` is true exactly when the layout holds a real `Balance<T>`. When it
    /// is false, the walk finds no balances, so skipping the walk loses nothing.
    #[test]
    fn layout_contains_balance_matches_the_layout(case in case()) {
        prop_assert_eq!(layout_contains_balance(&case.layout), case.contains_balance);
        if !case.contains_balance {
            let tracked = walk_contents(&case.layout, &case.value.bytes, true).unwrap();
            prop_assert!(tracked.balances.is_empty());
        }
    }

    /// The coin shortcut matches walking the coin's layout, and does not ask for the layout.
    #[test]
    fn coin_shortcut_matches_layout_walk_for_any_coin(
        id in object_id(),
        type_ in type_pool().prop_flat_map(|pool| balance_type(&pool)),
        amount in amount(),
        track in any::<bool>(),
    ) {
        let coin = MoveObject::new_coin(type_.clone(), SequenceNumber::new(), id, amount);
        let layout = struct_layout("Coin", vec![("id", uid()), ("balance", balance(&type_))]);
        let walked = walk_contents(&layout, coin.contents(), track).unwrap();
        let shortcut = object_contents(&coin, track, || {
            Err::<Option<&MoveTypeLayout>, _>("layout requested")
        })
        .unwrap()
        .unwrap();
        prop_assert_eq!(shortcut, walked);
    }

    /// `BalancesBefore` returns every UID and the summed totals of the objects recorded. The
    /// objects share a type pool, so totals also merge across objects.
    #[test]
    fn balances_before_combines_records(
        cases in type_pool().prop_flat_map(|pool| vec(object_case(pool), 0..=4)),
    ) {
        let mut before = BalancesBefore::new();
        let mut uids = BTreeSet::new();
        let mut expected = BalanceTotals::new();
        for (i, case) in cases.iter().enumerate() {
            let contents = walk_contents(&case.layout, &case.value.bytes, true).unwrap();
            before.record(ObjectID::from_single_byte(0x80 + i as u8), contents).unwrap();
            uids.extend(case.value.uids.iter().copied());
            merge_totals(&mut expected, &case.value.expected_totals()).unwrap();
        }
        let (totals, found) = before.finish().unwrap();
        prop_assert_eq!(found, uids);
        prop_assert_eq!(totals, expected);
    }

    #[test]
    fn truncated_bytes_are_an_error(case in case()) {
        let bytes = &case.value.bytes;
        for len in 0..bytes.len() {
            prop_assert!(walk_contents(&case.layout, &bytes[..len], false).is_err());
            prop_assert!(walk_contents(&case.layout, &bytes[..len], true).is_err());
        }
    }

    #[test]
    fn trailing_bytes_are_an_error(case in case(), extra in vec(any::<u8>(), 1..=4)) {
        let mut bytes = case.value.bytes.clone();
        bytes.extend(extra);
        prop_assert!(walk_contents(&case.layout, &bytes, false).is_err());
        prop_assert!(walk_contents(&case.layout, &bytes, true).is_err());
    }

    /// Random bytes may or may not parse, but never panic.
    #[test]
    fn random_bytes_do_not_panic(
        layout in type_pool().prop_flat_map(shape).prop_map(|s| layout(&s)),
        bytes in vec(any::<u8>(), 0..=64),
    ) {
        let _ = walk_contents(&layout, &bytes, false);
        let _ = walk_contents(&layout, &bytes, true);
    }
}
