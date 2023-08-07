// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test_DepV1=0x0 Test_DepV2=0x0 Test_V1=0x0 Test_V2=0x0 Test_V3=0x0 --accounts A

//# publish --upgradeable --sender A
module Test_DepV1::DepM1 {

    struct DepObj has key, store { id: sui::object::UID, v: u64 }

    public fun foo(ctx: &mut sui::tx_context::TxContext) {
        sui::transfer::share_object(DepObj { id: sui::object::new(ctx), v: 42 });
    }

    public fun mod_obj(o: &mut DepObj) {
        o.v = o.v - 1;
    }
}


//# upgrade --package Test_DepV1 --upgrade-capability 1,1 --sender A
module Test_DepV2::DepM1 {

    struct DepObj has key, store { id: sui::object::UID, v: u64 }

    public fun foo(ctx: &mut sui::tx_context::TxContext) {
        sui::transfer::share_object(DepObj { id: sui::object::new(ctx), v: 7 });
    }

    public fun mod_obj(o: &mut DepObj) {
        o.v = o.v - 2;
    }
}


//# publish --upgradeable --dependencies Test_DepV1 --sender A
module Test_V1::M1 {
    use Test_DepV1::DepM1;

    public entry fun bar(ctx: &mut sui::tx_context::TxContext) {
        DepM1::foo(ctx);
    }

    public fun mod_dep_obj(o: &mut DepM1::DepObj) {
        DepM1::mod_obj(o);
    }
}

//# upgrade --package Test_V1 --upgrade-capability 3,1 --dependencies Test_DepV1 --sender A
module Test_V2::M1 {
    use Test_DepV1::DepM1;

    public entry fun bar(ctx: &mut sui::tx_context::TxContext) {
        DepM1::foo(ctx);
    }

    public fun mod_dep_obj(o: &mut DepM1::DepObj) {
        DepM1::mod_obj(o);
    }
}

//# upgrade --package Test_V2 --upgrade-capability 3,1 --dependencies Test_DepV2 --sender A
module Test_V3::M1 {
    use Test_DepV2::DepM1;

    public entry fun bar(ctx: &mut sui::tx_context::TxContext) {
        DepM1::foo(ctx);
    }

    public fun mod_dep_obj(o: &mut DepM1::DepObj) {
        DepM1::mod_obj(o);
    }
}


//# run Test_DepV1::DepM1::foo

//# view-object 6,0

// call functions from two different versions of the same module modifying the same object
//# programmable --sender A --inputs object(6,0)
//> 0: Test_DepV1::DepM1::mod_obj(Input(0));
//> 1: Test_DepV2::DepM1::mod_obj(Input(0));

//# view-object 6,0

// call functions from two different versions of the same module modifying the same object defined
// in the same version of the dependent module
//# programmable --sender A --inputs object(6,0)
//> 0: Test_V1::M1::mod_dep_obj(Input(0));
//> 1: Test_V2::M1::mod_dep_obj(Input(0));

//# view-object 6,0

// call functions from two different versions of the same module modifying the same object defined
// in different versions of the dependent module
//# programmable --sender A --inputs object(6,0)
//> 0: Test_V2::M1::mod_dep_obj(Input(0));
//> 1: Test_V3::M1::mod_dep_obj(Input(0));

//# view-object 6,0
