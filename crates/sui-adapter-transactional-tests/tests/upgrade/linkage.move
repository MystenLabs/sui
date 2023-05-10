// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test_V2=0x0 Test_V1=0x0 Test_DepV1=0x0 Test_DepV2=0x0 --accounts A

//# publish --upgradeable --sender A
module Test_DepV1::DepM1 {
    struct DepObj has key, store { id: sui::object::UID, v: u64 }
    public fun mod_obj(o: &mut DepObj) {
        o.v = 0;
    }
}

//# upgrade --package Test_DepV1 --upgrade-capability 1,1 --sender A
module Test_DepV2::DepM1 {
    struct DepObj has key, store { id: sui::object::UID, v: u64 }
    public fun mod_obj(o: &mut DepObj) {
        o.v = 0;
    }

    public fun only_defined(o: &mut DepObj) {
        o.v = 1
    }
}

//# publish --upgradeable --dependencies Test_DepV1 --sender A
module Test_V1::M1 {
    use Test_DepV1::DepM1;
    public fun mod_dep_obj(o: &mut DepM1::DepObj) {
        DepM1::mod_obj(o);
    }
}

//# upgrade --package Test_V1 --upgrade-capability 3,1 --dependencies Test_DepV2 --sender A
module Test_V2::M1 {
    use Test_DepV2::DepM1;

    public fun mod_dep_obj(o: &mut DepM1::DepObj) {
        DepM1::only_defined(o);
    }
}
