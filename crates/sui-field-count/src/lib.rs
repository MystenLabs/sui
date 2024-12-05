// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use sui_field_count_derive::*;

pub trait FieldCount {
    const FIELD_COUNT: usize;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_field_count() {
        #[derive(FieldCount)]
        struct EmptyStruct {}
        #[derive(FieldCount)]
        struct BasicStruct {
            _field1: String,
            _field2: i32,
            _field3: bool,
        }

        assert_eq!(BasicStruct::FIELD_COUNT, 3);
        assert_eq!(EmptyStruct::FIELD_COUNT, 0);
    }

    #[test]
    fn test_lifetime_field_count() {
        #[derive(FieldCount)]
        struct LifetimeStruct<'a> {
            _field1: &'a str,
            _field2: &'a [u8],
            _field3: &'a Vec<String>,
        }

        assert_eq!(LifetimeStruct::FIELD_COUNT, 3);
    }

    #[test]
    fn test_generic_type_field_count() {
        #[derive(FieldCount)]
        struct GenericStruct<T> {
            _field1: T,
            _field2: Vec<T>,
            _field3: Option<T>,
        }

        assert_eq!(GenericStruct::<String>::FIELD_COUNT, 3);
        assert_eq!(GenericStruct::<i32>::FIELD_COUNT, 3);
    }

    #[test]
    fn test_where_clause_field_count() {
        #[derive(FieldCount)]
        struct WhereStruct<T>
        where
            T: Clone,
        {
            _field1: T,
            _field2: Vec<T>,
        }

        assert_eq!(WhereStruct::<String>::FIELD_COUNT, 2);
        assert_eq!(WhereStruct::<i32>::FIELD_COUNT, 2);
    }
}
