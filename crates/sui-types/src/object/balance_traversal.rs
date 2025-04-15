// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use move_core_types::{
    annotated_visitor::{self, StructDriver, Traversal, ValueDriver},
    language_storage::{StructTag, TypeTag},
};

use crate::balance::Balance;

/// Traversal to gather the total balances of all coin types visited.
#[derive(Default)]
pub(crate) struct BalanceTraversal {
    balances: BTreeMap<TypeTag, u64>,
}

/// Helper traversal to accumulate the values of all u64s visited. Used by `BalanceTraversal` to
/// get the value of a `Balance` struct's field.
#[derive(Default)]
struct Accumulator {
    total: u64,
}

impl BalanceTraversal {
    /// Consume the traversal to get at its balance mapping.
    pub(crate) fn finish(self) -> BTreeMap<TypeTag, u64> {
        self.balances
    }
}

impl<'b, 'l> Traversal<'b, 'l> for BalanceTraversal {
    type Error = annotated_visitor::Error;

    fn traverse_struct(
        &mut self,
        driver: &mut StructDriver<'_, 'b, 'l>,
    ) -> Result<(), Self::Error> {
        let Some(coin_type) = is_balance(&driver.struct_layout().type_) else {
            // Not a balance, search recursively for balances among fields.
            while driver.next_field(self)?.is_some() {}
            return Ok(());
        };

        let mut acc = Accumulator::default();
        while driver.next_field(&mut acc)?.is_some() {}
        *self.balances.entry(coin_type).or_default() += acc.total;
        Ok(())
    }
}

impl<'b, 'l> Traversal<'b, 'l> for Accumulator {
    type Error = annotated_visitor::Error;
    fn traverse_u64(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: u64,
    ) -> Result<(), Self::Error> {
        self.total += value;
        Ok(())
    }
}

/// Returns `Some(T)` if the struct is a `sui::balance::Balance<T>`, and `None` otherwise.
fn is_balance(s: &StructTag) -> Option<TypeTag> {
    (Balance::is_balance(s) && s.type_params.len() == 1).then(|| s.type_params[0].clone())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::id::UID;

    use super::*;

    use move_core_types::{
        account_address::AccountAddress, annotated_value as A, identifier::Identifier,
        language_storage::StructTag,
    };

    #[test]
    fn test_traverse_balance() {
        let layout = bal_t("0x42::foo::Bar");
        let value = bal_v("0x42::foo::Bar", 42);

        let bytes = serialize(value.clone());

        let mut visitor = BalanceTraversal::default();
        A::MoveValue::visit_deserialize(&bytes, &layout, &mut visitor).unwrap();
        let balances = visitor.finish();

        assert_eq!(balances, BTreeMap::from([(type_("0x42::foo::Bar"), 42)]));
    }

    #[test]
    fn test_traverse_coin() {
        let layout = coin_t("0x42::foo::Bar");
        let value = coin_v("0x42::foo::Bar", "0x101", 42);

        let bytes = serialize(value.clone());

        let mut visitor = BalanceTraversal::default();
        A::MoveValue::visit_deserialize(&bytes, &layout, &mut visitor).unwrap();
        let balances = visitor.finish();

        assert_eq!(balances, BTreeMap::from([(type_("0x42::foo::Bar"), 42)]));
    }

    #[test]
    fn test_traverse_nested() {
        use A::MoveTypeLayout as T;

        let layout = layout_(
            "0xa::foo::Bar",
            vec![
                ("b", bal_t("0x42::baz::Qux")),
                ("c", coin_t("0x42::baz::Qux")),
                ("d", T::Vector(Box::new(coin_t("0x42::quy::Frob")))),
            ],
        );

        let value = value_(
            "0xa::foo::Bar",
            vec![
                ("b", bal_v("0x42::baz::Qux", 42)),
                ("c", coin_v("0x42::baz::Qux", "0x101", 43)),
                (
                    "d",
                    A::MoveValue::Vector(vec![
                        coin_v("0x42::quy::Frob", "0x102", 44),
                        coin_v("0x42::quy::Frob", "0x103", 45),
                    ]),
                ),
            ],
        );

        let bytes = serialize(value.clone());

        let mut visitor = BalanceTraversal::default();
        A::MoveValue::visit_deserialize(&bytes, &layout, &mut visitor).unwrap();
        let balances = visitor.finish();

        assert_eq!(
            balances,
            BTreeMap::from([
                (type_("0x42::baz::Qux"), 42 + 43),
                (type_("0x42::quy::Frob"), 44 + 45),
            ])
        );
    }

    #[test]
    fn test_traverse_primitive() {
        use A::MoveTypeLayout as T;

        let layout = T::U64;
        let value = A::MoveValue::U64(42);
        let bytes = serialize(value.clone());

        let mut visitor = BalanceTraversal::default();
        A::MoveValue::visit_deserialize(&bytes, &layout, &mut visitor).unwrap();
        let balances = visitor.finish();

        assert_eq!(balances, BTreeMap::from([]));
    }

    #[test]
    fn test_traverse_fake_balance() {
        use A::MoveTypeLayout as T;

        let layout = layout_(
            "0xa::foo::Bar",
            vec![
                ("b", bal_t("0x42::baz::Qux")),
                ("c", coin_t("0x42::baz::Qux")),
                (
                    "d",
                    layout_(
                        // Fake balance
                        "0x3::balance::Balance<0x42::baz::Qux>",
                        vec![("value", T::U64)],
                    ),
                ),
            ],
        );

        let value = value_(
            "0xa::foo::Bar",
            vec![
                ("b", bal_v("0x42::baz::Qux", 42)),
                ("c", coin_v("0x42::baz::Qux", "0x101", 43)),
                (
                    "d",
                    value_(
                        "0x3::balance::Balance<0x42::baz::Qux>",
                        vec![("value", A::MoveValue::U64(44))],
                    ),
                ),
            ],
        );

        let bytes = serialize(value.clone());

        let mut visitor = BalanceTraversal::default();
        A::MoveValue::visit_deserialize(&bytes, &layout, &mut visitor).unwrap();
        let balances = visitor.finish();

        assert_eq!(
            balances,
            BTreeMap::from([(type_("0x42::baz::Qux"), 42 + 43),])
        );
    }

    /// Create a UID Move Value for test purposes.
    fn uid_(addr: &str) -> A::MoveValue {
        value_(
            "0x2::object::UID",
            vec![(
                "id",
                value_(
                    "0x2::object::ID",
                    vec![(
                        "bytes",
                        A::MoveValue::Address(AccountAddress::from_str(addr).unwrap()),
                    )],
                ),
            )],
        )
    }

    /// Create a Balance value for testing purposes.
    fn bal_v(tag: &str, value: u64) -> A::MoveValue {
        value_(
            &format!("0x2::balance::Balance<{tag}>"),
            vec![("value", A::MoveValue::U64(value))],
        )
    }

    /// Create a Coin value for testing purposes.
    fn coin_v(tag: &str, id: &str, value: u64) -> A::MoveValue {
        value_(
            &format!("0x2::coin::Coin<{tag}>"),
            vec![("id", uid_(id)), ("balance", bal_v(tag, value))],
        )
    }

    /// Create a Balance layout for testing purposes.
    fn bal_t(tag: &str) -> A::MoveTypeLayout {
        layout_(
            &format!("0x2::balance::Balance<{tag}>"),
            vec![("value", A::MoveTypeLayout::U64)],
        )
    }

    /// Create a Coin layout for testing purposes.
    fn coin_t(tag: &str) -> A::MoveTypeLayout {
        layout_(
            &format!("0x2::coin::Coin<{tag}>"),
            vec![
                ("id", A::MoveTypeLayout::Struct(Box::new(UID::layout()))),
                ("balance", bal_t(tag)),
            ],
        )
    }

    /// Create a struct value for test purposes.
    fn value_(rep: &str, fields: Vec<(&str, A::MoveValue)>) -> A::MoveValue {
        let type_ = StructTag::from_str(rep).unwrap();
        let fields = fields
            .into_iter()
            .map(|(name, value)| (Identifier::new(name).unwrap(), value))
            .collect();

        A::MoveValue::Struct(A::MoveStruct::new(type_, fields))
    }

    // Create a type tag for test purposes.
    fn type_(rep: &str) -> TypeTag {
        TypeTag::from_str(rep).unwrap()
    }

    /// Create a struct layout for test purposes.
    fn layout_(rep: &str, fields: Vec<(&str, A::MoveTypeLayout)>) -> A::MoveTypeLayout {
        let type_ = StructTag::from_str(rep).unwrap();
        let fields = fields
            .into_iter()
            .map(|(name, layout)| A::MoveFieldLayout::new(Identifier::new(name).unwrap(), layout))
            .collect();

        A::MoveTypeLayout::Struct(Box::new(A::MoveStructLayout { type_, fields }))
    }

    /// BCS encode Move value.
    fn serialize(value: A::MoveValue) -> Vec<u8> {
        value.clone().undecorate().simple_serialize().unwrap()
    }
}
