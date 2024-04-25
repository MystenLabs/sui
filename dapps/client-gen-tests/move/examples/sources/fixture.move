module examples::fixture {
    use std::ascii;
    use std::string;
    use std::option::Option;

    use sui::object::{Self, ID, UID};
    use sui::url::Url;
    use sui::balance::Balance;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::sui::SUI;

    use examples::other_module::{Self, StructFromOtherModule};

    const ASDF: u64 = 123;

    public struct Dummy has store { }

    public struct WithGenericField<T: store> has key {
        id: UID,
        generic_field: T,
    }

    public struct Bar has store, copy, drop {
        value: u64
    }

    public struct WithTwoGenerics<T: store + drop, U: store + drop> has store, drop {
        generic_field_1: T,
        generic_field_2: U
    }

    public struct Foo<T: store + drop> has key {
        id: UID,
        generic: T,
        reified_primitive_vec: vector<u64>,
        reified_object_vec: vector<Bar>,
        generic_vec: vector<T>,
        generic_vec_nested: vector<WithTwoGenerics<T, u8>>,
        two_generics: WithTwoGenerics<T, Bar>,
        two_generics_reified_primitive: WithTwoGenerics<u16, u64>,
        two_generics_reified_object: WithTwoGenerics<Bar, Bar>,
        two_generics_nested: WithTwoGenerics<T, WithTwoGenerics<u8, u8>>,
        two_generics_reified_nested: WithTwoGenerics<Bar, WithTwoGenerics<u8, u8>>,
        two_generics_nested_vec: vector<WithTwoGenerics<Bar, vector<WithTwoGenerics<T, u8>>>>,
        dummy: Dummy,
        other: StructFromOtherModule,
    }

    public struct WithSpecialTypes<phantom T, U: store> has key, store {
        id: UID,
        string: string::String,
        ascii_string: ascii::String,
        url: Url,
        id_field: ID,
        uid: UID,
        balance: Balance<SUI>,
        option: Option<u64>,
        option_obj: Option<Bar>,
        option_none: Option<u64>,
        balance_generic: Balance<T>,
        option_generic: Option<U>,
        option_generic_none: Option<U>,
    }

    public struct WithSpecialTypesAsGenerics<
        T0: store, T1: store, T2: store, T3: store, T4: store, T5: store, T6: store, T7: store
    > has key, store {
        id: UID,
        string: T0,
        ascii_string: T1,
        url: T2,
        id_field: T3,
        uid: T4,
        balance: T5,
        option: T6,
        option_none: T7,
    }

    public struct WithSpecialTypesInVectors<T: store> has key, store {
        id: UID,
        string: vector<string::String>,
        ascii_string: vector<ascii::String>,
        id_field: vector<ID>,
        bar: vector<Bar>,
        option: vector<Option<u64>>,
        option_generic: vector<Option<T>>,
    }

    public fun create_with_generic_field<T: store>(generic_field: T, tx_context: &mut TxContext) {
        let obj = WithGenericField {
            id: object::new(tx_context),
            generic_field,
        };
        transfer::transfer(obj, tx_context::sender(tx_context));
    }

    public fun create_bar(value: u64): Bar {
        Bar { value }
    }

    public fun create_with_two_generics<T: store + drop, U: store + drop>(
        generic_field_1: T,
        generic_field_2: U,
    ): WithTwoGenerics<T, U> {
        WithTwoGenerics {
            generic_field_1,
            generic_field_2
        }
    }

    public fun create_foo<T: store + drop, U: store + drop>(
        generic: T,
        reified_primitive_vec: vector<u64>,
        reified_object_vec: vector<Bar>,
        generic_vec: vector<T>,
        generic_vec_nested: vector<WithTwoGenerics<T, u8>>,
        two_generics: WithTwoGenerics<T, Bar>,
        two_generics_reified_primitive: WithTwoGenerics<u16, u64>,
        two_generics_reified_object: WithTwoGenerics<Bar, Bar>,
        two_generics_nested: WithTwoGenerics<T, WithTwoGenerics<u8, u8>>,
        two_generics_reified_nested: WithTwoGenerics<Bar, WithTwoGenerics<u8, u8>>,
        two_generics_nested_vec: vector<WithTwoGenerics<Bar, vector<WithTwoGenerics<T, u8>>>>,
        _obj_ref: &Bar,
        tx_context: &mut TxContext
    ) {
        let obj = Foo {
            id: object::new(tx_context),
            generic,
            reified_primitive_vec,
            reified_object_vec,
            generic_vec,
            generic_vec_nested,
            two_generics,
            two_generics_reified_primitive,
            two_generics_reified_object,
            two_generics_nested,
            two_generics_reified_nested,
            two_generics_nested_vec,
            dummy: Dummy {},
            other: other_module::new(),
        };
        transfer::transfer(obj, tx_context::sender(tx_context));
    }

    public fun create_special<T, U: store>(
        string: string::String,
        ascii_string: ascii::String,
        url: Url,
        id_field: ID,
        uid: UID,
        balance: Balance<SUI>,
        option: Option<u64>,
        option_obj: Option<Bar>,
        option_none: Option<u64>,
        balance_generic: Balance<T>,
        option_generic: Option<U>,
        option_generic_none: Option<U>,
        tx_context: &mut TxContext
    ) {
        let obj = WithSpecialTypes {
            id: object::new(tx_context),
            string,
            ascii_string,
            url,
            id_field,
            uid,
            balance,
            option,
            option_obj,
            option_none,
            balance_generic,
            option_generic,
            option_generic_none,
        };
        transfer::transfer(obj, tx_context::sender(tx_context));
    }

    public fun create_special_as_generics<
        T0: store, T1: store, T2: store, T3: store, T4: store, T5: store, T6: store, T7: store
    >(
        string: T0,
        ascii_string: T1,
        url: T2,
        id_field: T3,
        uid: T4,
        balance: T5,
        option: T6,
        option_none: T7,
        tx_context: &mut TxContext
    ) {
        let obj = WithSpecialTypesAsGenerics {
            id: object::new(tx_context),
            string,
            ascii_string,
            url,
            id_field,
            uid,
            balance,
            option,
            option_none,
        };
        transfer::transfer(obj, tx_context::sender(tx_context));
    }

    public fun create_special_in_vectors<T: store>(
        string: vector<string::String>,
        ascii_string: vector<ascii::String>,
        id_field: vector<ID>,
        bar: vector<Bar>,
        option: vector<Option<u64>>,
        option_generic: vector<Option<T>>,
        tx_context: &mut TxContext
    ) {
        let obj = WithSpecialTypesInVectors{
            id: object::new(tx_context),
            string,
            ascii_string,
            id_field,
            bar,
            option,
            option_generic,
        };
        transfer::transfer(obj, tx_context::sender(tx_context));
    }
}