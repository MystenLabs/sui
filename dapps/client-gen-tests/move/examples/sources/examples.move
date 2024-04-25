module examples::examples {
    use std::ascii;
    use std::string;
    use std::option::Option;
    use sui::object::{Self, ID, UID};
    use sui::tx_context::TxContext;
    use sui::transfer;

    public struct ExampleStruct has drop, store { }

    public struct SpecialTypesStruct has key {
        id: UID,
        ascii_string: ascii::String,
        utf8_string: string::String,
        vector_of_u64: vector<u64>,
        vector_of_objects: vector<ExampleStruct>,
        id_field: ID,
        address: address,
        option_some: Option<u64>,
        option_none: Option<u64>,
    }

    public fun create_example_struct(): ExampleStruct {
        ExampleStruct { }
    }

    public fun special_types(
        ascii_string: ascii::String,
        utf8_string: string::String,
        vector_of_u64: vector<u64>,
        vector_of_objects: vector<ExampleStruct>,
        id_field: ID,
        address: address,
        option_some: Option<u64>,
        option_none: Option<u64>,
        ctx: &mut TxContext
     ) {
        let obj = SpecialTypesStruct {
            id: object::new(ctx),
            ascii_string,
            utf8_string,
            vector_of_u64,
            vector_of_objects,
            id_field,
            address,
            option_some,
            option_none,
        };
        transfer::share_object(obj);
     }
}