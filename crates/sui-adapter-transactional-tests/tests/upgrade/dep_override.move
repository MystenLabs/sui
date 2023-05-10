// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test_DepDepV1=0x0 Test_DepDepV2=0x0 Test_DepDepV3=0x0 Test_DepV1=0x0 Test_DepV2=0x0 Test_V1=0x0 Test_V2=0x0 Test_V3=0x0 Test_V4=0x0 --accounts A


// 3 versions of the transitive dependency


//# publish --upgradeable --sender A
module Test_DepDepV1::DepDepM1 {

    struct Obj has key, store { id: sui::object::UID, v: u64 }

    public fun foo(ctx: &mut sui::tx_context::TxContext) {
        sui::transfer::share_object(Obj { id: sui::object::new(ctx), v: 42 })
    }
}

//# upgrade --package Test_DepDepV1 --upgrade-capability 1,1 --sender A
module Test_DepDepV2::DepDepM1 {

    struct Obj has key, store { id: sui::object::UID, v: u64 }

    public fun foo(ctx: &mut sui::tx_context::TxContext) {
        sui::transfer::share_object(Obj { id: sui::object::new(ctx), v: 7 })
    }
}

//# upgrade --package Test_DepDepV2 --upgrade-capability 1,1 --sender A
module Test_DepDepV3::DepDepM1 {

    struct Obj has key, store { id: sui::object::UID, v: u64 }

    public fun foo(ctx: &mut sui::tx_context::TxContext) {
        sui::transfer::share_object(Obj { id: sui::object::new(ctx), v: 0 })
    }
}


// 2 versions of the direct dependency


//# publish --upgradeable --dependencies Test_DepDepV1 --sender A
module Test_DepV1::DepM1 {
    use Test_DepDepV1::DepDepM1;

    public fun bar(ctx: &mut sui::tx_context::TxContext) { DepDepM1::foo(ctx) }
}

//# upgrade --package Test_DepV1 --upgrade-capability 4,1 --dependencies Test_DepDepV2 --sender A
module Test_DepV2::DepM1 {
    use Test_DepDepV2::DepDepM1;

    public fun bar(ctx: &mut sui::tx_context::TxContext) { DepDepM1::foo(ctx)  }
}


// 3 versions of the root package


//# publish --upgradeable --dependencies Test_DepV1 Test_DepDepV1 --sender A
module Test_V1::M1 {
    use Test_DepV1::DepM1;

    public entry fun baz(ctx: &mut sui::tx_context::TxContext) { DepM1::bar(ctx) }
}

// override direct dependency

//# upgrade --package Test_V1 --upgrade-capability 6,1 --dependencies Test_DepV2 Test_DepDepV2 --sender A
module Test_V2::M1 {
    use Test_DepV2::DepM1;

    public entry fun baz(ctx: &mut sui::tx_context::TxContext) { DepM1::bar(ctx) }
}

// override indirect dependency

//# upgrade --package Test_V2 --upgrade-capability 6,1 --dependencies Test_DepV1 Test_DepDepV3 --sender A
module Test_V3::M1 {
    use Test_DepV1::DepM1;

    public entry fun baz(ctx: &mut sui::tx_context::TxContext) { DepM1::bar(ctx) }
}

//# run Test_V1::M1::baz

//# view-object 9,0

//# run Test_V2::M1::baz

//# view-object 11,0

//# run Test_V3::M1::baz

//# view-object 13,0

// call same function from two different module versions but defined in both modules (should produce
// different result due to overrides)
//# programmable --sender A
//> 0: Test_V2::M1::baz();
//> 1: Test_V3::M1::baz();

//# view-object 15,0

//# view-object 15,1


// expected upgrade errors

// missing direct dependency (should fail)

//# upgrade --package Test_V3 --upgrade-capability 6,1 --dependencies Test_DepDepV1 --sender A
module Test_V4::M1 {
    use Test_DepV1::DepM1;
    public entry fun baz(ctx: &mut sui::tx_context::TxContext) { DepM1::bar(ctx) }
}

// missing indirect dependency (should fail)

//# upgrade --package Test_V3 --upgrade-capability 6,1 --dependencies Test_DepV2 --sender A
module Test_V4::M1 {
    use Test_DepV2::DepM1;
    public entry fun baz(ctx: &mut sui::tx_context::TxContext) { DepM1::bar(ctx) }
}

// downgrade indirect dependency (should fail)

//# upgrade --package Test_V3 --upgrade-capability 6,1 --dependencies Test_DepV2 Test_DepDepV1 --sender A
module Test_V4::M1 {
    use Test_DepV2::DepM1;
    public entry fun baz(ctx: &mut sui::tx_context::TxContext) { DepM1::bar(ctx) }
}
