// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses tto=0x0

//# publish
module tto::M1 {
    use sui::transfer::Receiving;

    public struct A has key, store {
        id: UID,
    }

    public struct B has key, store {
        id: UID,
    }

    public fun start(ctx: &mut TxContext) {
        let a = A { id: object::new(ctx) };
        let a_address = object::id_address(&a);
        let b = B { id: object::new(ctx) };
        transfer::public_transfer(a, tx_context::sender(ctx));
        transfer::public_transfer(b, a_address);
    }

    public fun call_immut_ref(_parent: &mut A, _x: &Receiving<B>) { }
    public fun call_mut_ref(_parent: &mut A, _x: &mut Receiving<B>) { }
    public fun call_mut_ref_ret(_parent: &mut A, x: &mut Receiving<B>): &mut Receiving<B> { x  }
    public fun call_mut_ref_immut_ret(_parent: &mut A, x: &mut Receiving<B>): &Receiving<B> { x  }
    public fun immut_immut_ref(_x: &Receiving<B>, _y: &Receiving<B>) { }
    public fun immut_mut_ref(_x: &Receiving<B>, _y: &mut Receiving<B>) { }
    public fun mut_immut_ref(_x: &mut Receiving<B>, _y: &Receiving<B>) { }
    public fun mut_mut_ref(_x: &mut Receiving<B>, _y: &mut Receiving<B>) { }
    public fun take_mut_ref(_x: Receiving<B>, _y: &mut Receiving<B>) { }
    public fun take_immut_ref(_x: Receiving<B>, _y: &Receiving<B>) { }
    public fun immut_ref_take(_x: &Receiving<B>, _y: Receiving<B>) { }
    public fun mut_ref_take(_x: &mut Receiving<B>, _y: Receiving<B>) { }
    public fun double_take(_x: Receiving<B>, _y: Receiving<B>) { }
}

//# run tto::M1::start

//# view-object 2,0

//# view-object 2,1

//# run tto::M1::call_mut_ref --args object(2,0) receiving(2,1)

//# run tto::M1::call_immut_ref --args object(2,0) receiving(2,1)

//# run tto::M1::call_mut_ref_ret --args object(2,0) receiving(2,1)

//# run tto::M1::call_mut_ref_immut_ret --args object(2,0) receiving(2,1)

//# programmable --inputs receiving(2,1)
//> tto::M1::immut_immut_ref(Input(0), Input(0))

//# programmable --inputs receiving(2,1)
//> tto::M1::immut_mut_ref(Input(0), Input(0))

//# programmable --inputs receiving(2,1)
//> tto::M1::mut_immut_ref(Input(0), Input(0))

//# programmable --inputs receiving(2,1)
//> tto::M1::mut_mut_ref(Input(0), Input(0))

//# programmable --inputs receiving(2,1)
//> tto::M1::take_mut_ref(Input(0), Input(0))

//# programmable --inputs receiving(2,1)
//> tto::M1::take_immut_ref(Input(0), Input(0))

//# programmable --inputs receiving(2,1)
//> tto::M1::immut_ref_take(Input(0), Input(0))

//# programmable --inputs receiving(2,1)
//> tto::M1::mut_ref_take(Input(0), Input(0))

//# programmable --inputs receiving(2,1)
//> tto::M1::double_take(Input(0), Input(0))
