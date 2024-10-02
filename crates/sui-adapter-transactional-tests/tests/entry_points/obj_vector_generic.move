// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests vector of objects where operations involve generics (type parameters)

//# init --addresses Test=0x0 --accounts A --shared-object-deletion true

//# publish
module Test::M {
    public struct ObjAny<phantom Any> has key, store {
        id: UID,
        value: u64
    }

    public struct AnotherObjAny<phantom Any> has key {
        id: UID,
        value: u64
    }

    public struct Any {}

    public entry fun mint_any<Any>(v: u64, ctx: &mut TxContext) {
        transfer::public_transfer(
            ObjAny<Any> {
                id: object::new(ctx),
                value: v,
            },
            tx_context::sender(ctx),
        )
    }

    public entry fun mint_another_any<Any>(v: u64, ctx: &mut TxContext) {
        transfer::transfer(
            AnotherObjAny<Any> {
                id: object::new(ctx),
                value: v,
            },
            tx_context::sender(ctx),
        )
    }

    public entry fun mint_child_any<Any>(v: u64, parent: &mut ObjAny<Any>, ctx: &mut TxContext) {
        sui::dynamic_object_field::add(
            &mut parent.id,
            0,
            ObjAny<Any> {
                id: object::new(ctx),
                value: v,
            },
        )
    }

    public entry fun mint_shared_any<Any>(v: u64, ctx: &mut TxContext) {
        transfer::public_share_object(
            ObjAny<Any> {
                id: object::new(ctx),
                value: v,
            }
        )
    }

    public entry fun obj_vec_destroy_any<Any>(mut v: vector<ObjAny<Any>>, _: &mut TxContext) {
        assert!(vector::length(&v) == 1, 0);
        let ObjAny<Any> {id, value} = vector::pop_back(&mut v);
        assert!(value == 42, 0);
        object::delete(id);
        vector::destroy_empty(v);
    }

    public entry fun two_obj_vec_destroy_any<Any>(mut v: vector<ObjAny<Any>>, _: &mut TxContext) {
        assert!(vector::length(&v) == 2, 0);
        let ObjAny<Any> {id, value} = vector::pop_back(&mut v);
        assert!(value == 42, 0);
        object::delete(id);
        let ObjAny<Any> {id, value} = vector::pop_back(&mut v);
        assert!(value == 7, 0);
        object::delete(id);
        vector::destroy_empty(v);
    }

    public entry fun same_objects_any<Any>(o: ObjAny<Any>, mut v: vector<ObjAny<Any>>, _: &mut TxContext) {
        let ObjAny<Any> {id, value} = o;
        assert!(value == 42, 0);
        object::delete(id);
        let ObjAny<Any> {id, value} = vector::pop_back(&mut v);
        assert!(value == 42, 0);
        object::delete(id);
        vector::destroy_empty(v);
    }

    public entry fun same_objects_ref_any<Any>(o: &ObjAny<Any>, mut v: vector<ObjAny<Any>>, _: &mut TxContext) {
        assert!(o.value == 42, 0);
        let ObjAny<Any> {id, value: _} = vector::pop_back(&mut v);
        object::delete(id);
        vector::destroy_empty(v);
    }

    public entry fun child_access_any<Any>(child: ObjAny<Any>, mut v: vector<ObjAny<Any>>, _: &mut TxContext) {
        let ObjAny<Any> {id, value} = child;
        assert!(value == 42, 0);
        object::delete(id);
        let ObjAny<Any> {id, value} = vector::pop_back(&mut v);
        assert!(value == 42, 0);
        object::delete(id);
        vector::destroy_empty(v);
    }

}

// create an object and pass it as a single element of a vector (success)

//# run Test::M::mint_any --sender A --type-args Test::M::Any --args 42

//# run Test::M::obj_vec_destroy_any --sender A --type-args Test::M::Any --args vector[object(2,0)]


// create a parent/child object pair, pass child by-value and parent as a single element of a vector
// to check if authentication works (success)

//# run Test::M::mint_any --sender A --type-args Test::M::Any --args 42

//# run Test::M::mint_child_any --sender A --type-args Test::M::Any --args 42 object(4,0)

//# run Test::M::child_access_any --sender A --type-args Test::M::Any --args object(4,0) vector[object(5,0)]


// create an object of one type and try to pass it as a single element of a vector whose elements
// are of different type (failure)

//# run Test::M::mint_another_any --type-args Test::M::Any --sender A --args 42

//# run Test::M::obj_vec_destroy_any --sender A --type-args Test::M::Any --args vector[object(7,0)]


// create two objects of different types and try to pass them both as elements of a vector (failure)

//# run Test::M::mint_another_any --sender A --type-args Test::M::Any --args 42

//# run Test::M::mint_any --sender A --type-args Test::M::Any --args 42

//# run Test::M::two_obj_vec_destroy_any --sender A --type-args Test::M::Any --args vector[object(9,0),object(10,0)]


// create a shared object and try to pass it as a single element of a vector

//# run Test::M::mint_shared_any --sender A --type-args Test::M::Any --args 42

//# run Test::M::obj_vec_destroy_any --sender A --type-args Test::M::Any --args vector[object(12,0)]


// create an object and pass it both by-value and as element of a vector

//# run Test::M::mint_any --sender A --type-args Test::M::Any --args 42

//# run Test::M::same_objects_any --sender A --type-args Test::M::Any --args object(14,0) vector[object(14,0)]


// create an object and pass it both by-reference and as element of a vector (failure)

//# run Test::M::mint_any --sender A --type-args Test::M::Any --args 42

//# run Test::M::same_objects_ref_any --sender A --type-args Test::M::Any --args object(16,0) vector[object(16,0)]
