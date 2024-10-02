// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module entry_point_vector::entry_point_vector {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use std::vector;

    public struct Obj has key, store {
        id: UID,
        value: u64
    }

    public struct AnotherObj has key {
        id: UID,
        value: u64
    }

    public struct ObjAny<phantom Any> has key, store {
        id: UID,
        value: u64
    }

    public struct AnotherObjAny<phantom Any> has key {
        id: UID,
        value: u64
    }

    public struct Any {}

    public entry fun mint(v: u64, ctx: &mut TxContext) {
        transfer::public_transfer(
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
        sui::dynamic_object_field::add(
            &mut parent.id, 0,
            Obj {
                id: object::new(ctx),
                value: v,
            },
        )
    }

    public entry fun mint_shared(v: u64, ctx: &mut TxContext) {
        transfer::public_share_object(
            Obj {
                id: object::new(ctx),
                value: v,
            }
        )
    }


    public entry fun prim_vec_len(v: vector<u64>, _: &mut TxContext) {
        assert!(vector::length(&v) == 2, 0);
    }

    public entry fun obj_vec_empty(v: vector<Obj>, _: &mut TxContext) {
        vector::destroy_empty(v);
    }

    public entry fun obj_vec_destroy(mut v: vector<Obj>, _: &mut TxContext) {
        assert!(vector::length(&v) == 1, 0);
        let Obj {id, value} = vector::pop_back(&mut v);
        assert!(value == 42, 0);
        object::delete(id);
        vector::destroy_empty(v);
    }

    public entry fun two_obj_vec_destroy(mut v: vector<Obj>, _: &mut TxContext) {
        assert!(vector::length(&v) == 2, 0);
        let Obj {id, value} = vector::pop_back(&mut v);
        assert!(value == 42, 0);
        object::delete(id);
        let Obj {id, value} = vector::pop_back(&mut v);
        assert!(value == 7, 0);
        object::delete(id);
        vector::destroy_empty(v);
    }

    public entry fun same_objects(o: Obj, mut v: vector<Obj>, _: &mut TxContext) {
        let Obj {id, value} = o;
        assert!(value == 42, 0);
        object::delete(id);
        let Obj {id, value} = vector::pop_back(&mut v);
        assert!(value == 42, 0);
        object::delete(id);
        vector::destroy_empty(v);
    }

    public entry fun same_objects_ref(o: &Obj, mut v: vector<Obj>, _: &mut TxContext) {
        assert!(o.value == 42, 0);
        let Obj {id, value: _} = vector::pop_back(&mut v);
        object::delete(id);
        vector::destroy_empty(v);
    }

    public entry fun child_access(child: Obj, mut v: vector<Obj>, _: &mut TxContext) {
        let Obj {id, value} = child;
        assert!(value == 42, 0);
        object::delete(id);
        let Obj {id, value} = vector::pop_back(&mut v);
        assert!(value == 42, 0);
        object::delete(id);
        vector::destroy_empty(v);
    }

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
        transfer::share_object(
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

    public entry fun type_param_vec_empty<T: key>(v: vector<T>, _: &mut TxContext) {
        vector::destroy_empty(v);
    }
}
