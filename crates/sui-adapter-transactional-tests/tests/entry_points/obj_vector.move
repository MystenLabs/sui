// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0 --accounts A

//# publish
module Test::M {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use std::vector;

    struct Obj has key {
        id: UID,
        value: u64
    }

    struct AnotherObj has key {
        id: UID,
        value: u64
    }

    public entry fun mint(v: u64, ctx: &mut TxContext) {
        transfer::transfer(
            Obj {
                id: object::new(ctx),
                value: v,
            },
            tx_context::sender(ctx),
        )
    }

    public entry fun mint_another(v: u64, ctx: &mut TxContext) {
        transfer::transfer(
            AnotherObj {
                id: object::new(ctx),
                value: v,
            },
            tx_context::sender(ctx),
        )
    }

    public entry fun mint_child(v: u64, parent: &mut Obj, ctx: &mut TxContext) {
        transfer::transfer_to_object(
            Obj {
                id: object::new(ctx),
                value: v,
            },
            parent,
        )
    }

    public entry fun mint_shared(v: u64, ctx: &mut TxContext) {
        transfer::share_object(
            Obj {
                id: object::new(ctx),
                value: v,
            }
        )
    }

    public entry fun prim_vec_len(v: vector<u64>, _: &mut TxContext) {
        assert!(vector::length(&v) == 2, 0);
    }

    public entry fun obj_vec_destroy(v: vector<Obj>, _: &mut TxContext) {
        assert!(vector::length(&v) == 1, 0);
        let Obj {id, value} = vector::pop_back(&mut v);
        assert!(value == 42, 0);
        object::delete(id);
        vector::destroy_empty(v);
    }

    public entry fun two_obj_vec_destroy(v: vector<Obj>, _: &mut TxContext) {
        assert!(vector::length(&v) == 2, 0);
        let Obj {id, value} = vector::pop_back(&mut v);
        assert!(value == 42, 0);
        object::delete(id);
        let Obj {id, value} = vector::pop_back(&mut v);
        assert!(value == 7, 0);
        object::delete(id);
        vector::destroy_empty(v);
    }

    public entry fun same_objects(o: Obj, v: vector<Obj>, _: &mut TxContext) {
        let Obj {id, value} = o;
        assert!(value == 42, 0);
        object::delete(id);
        let Obj {id, value} = vector::pop_back(&mut v);
        assert!(value == 42, 0);
        object::delete(id);
        vector::destroy_empty(v);
    }

    public entry fun same_objects_ref(o: &Obj, v: vector<Obj>, _: &mut TxContext) {
        assert!(o.value == 42, 0);
        let Obj {id, value: _} = vector::pop_back(&mut v);
        object::delete(id);
        vector::destroy_empty(v);
    }

    public entry fun child_access(child: Obj, v: vector<Obj>, _: &mut TxContext) {
        let Obj {id, value} = child;
        assert!(value == 42, 0);
        object::delete(id);
        let Obj {id, value} = vector::pop_back(&mut v);
        assert!(value == 42, 0);
        object::delete(id);
        vector::destroy_empty(v);
    }

    struct ObjAny<phantom Any> has key {
        id: UID,
        value: u64
    }

    struct AnotherObjAny<phantom Any> has key {
        id: UID,
        value: u64
    }

    struct Any {}

    public entry fun mint_any<Any>(v: u64, ctx: &mut TxContext) {
        transfer::transfer(
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
        transfer::transfer_to_object(
            ObjAny<Any> {
                id: object::new(ctx),
                value: v,
            },
            parent,
        )
    }

    public entry fun mint_shared_any<Any>(v: u64, ctx: &mut TxContext) {
        transfer::share_object(
            ObjAny<Any> {
                id: object::new(ctx),
                value: v,
            }
        )
    }

    public entry fun obj_vec_destroy_any<Any>(v: vector<ObjAny<Any>>, _: &mut TxContext) {
        assert!(vector::length(&v) == 1, 0);
        let ObjAny<Any> {id, value} = vector::pop_back(&mut v);
        assert!(value == 42, 0);
        object::delete(id);
        vector::destroy_empty(v);
    }

    public entry fun two_obj_vec_destroy_any<Any>(v: vector<ObjAny<Any>>, _: &mut TxContext) {
        assert!(vector::length(&v) == 2, 0);
        let ObjAny<Any> {id, value} = vector::pop_back(&mut v);
        assert!(value == 42, 0);
        object::delete(id);
        let ObjAny<Any> {id, value} = vector::pop_back(&mut v);
        assert!(value == 7, 0);
        object::delete(id);
        vector::destroy_empty(v);
    }

    public entry fun same_objects_any<Any>(o: ObjAny<Any>, v: vector<ObjAny<Any>>, _: &mut TxContext) {
        let ObjAny<Any> {id, value} = o;
        assert!(value == 42, 0);
        object::delete(id);
        let ObjAny<Any> {id, value} = vector::pop_back(&mut v);
        assert!(value == 42, 0);
        object::delete(id);
        vector::destroy_empty(v);
    }

    public entry fun same_objects_ref_any<Any>(o: &ObjAny<Any>, v: vector<ObjAny<Any>>, _: &mut TxContext) {
        assert!(o.value == 42, 0);
        let ObjAny<Any> {id, value: _} = vector::pop_back(&mut v);
        object::delete(id);
        vector::destroy_empty(v);
    }

    public entry fun child_access_any<Any>(child: ObjAny<Any>, v: vector<ObjAny<Any>>, _: &mut TxContext) {
        let ObjAny<Any> {id, value} = child;
        assert!(value == 42, 0);
        object::delete(id);
        let ObjAny<Any> {id, value} = vector::pop_back(&mut v);
        assert!(value == 42, 0);
        object::delete(id);
        vector::destroy_empty(v);
    }

}
// "positive" tests start here


//# run Test::M::prim_vec_len --sender A --args vector[7,42]

//# run Test::M::mint --sender A --args 42

//# run Test::M::obj_vec_destroy --sender A --args vector[object(107)]

//# run Test::M::mint --sender A --args 42

//# run Test::M::mint_child --sender A --args 42 object(110)

//# run Test::M::child_access --sender A --args object(110) vector[object(112)]


// "negative" tests start here


//# run Test::M::mint_another --sender A --args 42

//# run Test::M::obj_vec_destroy --sender A --args vector[object(115)]

//# run Test::M::mint_another --sender A --args 42

//# run Test::M::mint --sender A --args 42

//# run Test::M::two_obj_vec_destroy --sender A --args vector[object(118),object(120)]

//# run Test::M::mint_shared --sender A --args 42

//# run Test::M::obj_vec_destroy --sender A --args vector[object(123)]

//# run Test::M::mint --sender A --args 42

//# run Test::M::same_objects --sender A --args object(126) vector[object(126)]

//# run Test::M::mint --sender A --args 42

//# run Test::M::same_objects_ref --sender A --args object(128) vector[object(128)]


// "positive" tests start here (for generic vectors)


//# run Test::M::mint_any --sender A --type-args Test::M::Any --args 42

//# run Test::M::obj_vec_destroy_any --sender A --type-args Test::M::Any --args vector[object(132)]

//# run Test::M::mint_any --sender A --type-args Test::M::Any --args 42

//# run Test::M::mint_child_any --sender A --type-args Test::M::Any --args 42 object(135)

//# run Test::M::child_access_any --sender A --type-args Test::M::Any --args object(135) vector[object(137)]


// "negative" tests start here (for generic vectors)


//# run Test::M::mint_another_any --type-args Test::M::Any --sender A --args 42

//# run Test::M::obj_vec_destroy_any --sender A --type-args Test::M::Any --args vector[object(140)]

//# run Test::M::mint_another_any --sender A --type-args Test::M::Any --args 42

//# run Test::M::mint_any --sender A --type-args Test::M::Any --args 42

//# run Test::M::two_obj_vec_destroy_any --sender A --type-args Test::M::Any --args vector[object(143),object(145)]

//# run Test::M::mint_shared_any --sender A --type-args Test::M::Any --args 42

//# run Test::M::obj_vec_destroy_any --sender A --type-args Test::M::Any --args vector[object(148)]

//# run Test::M::mint_any --sender A --type-args Test::M::Any --args 42

//# run Test::M::same_objects_any --sender A --type-args Test::M::Any --args object(151) vector[object(151)]

//# run Test::M::mint_any --sender A --type-args Test::M::Any --args 42

//# run Test::M::same_objects_ref_any --sender A --type-args Test::M::Any --args object(154) vector[object(154)]
