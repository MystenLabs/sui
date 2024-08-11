// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test1=0x0 Test2=0x0 Test3=0x0 --accounts A

//# publish --upgradeable --sender A
module Test1::M1 {
    public fun f1() { 
        abort 0
    }
}


//# upgrade --package Test1 --upgrade-capability 1,1 --sender A
module Test2::M1 {
    public fun f1() { 
        abort 0
    }
}

//# upgrade --package Test2 --upgrade-capability 1,1 --sender A
module Test3::M1 {
    public fun f1() { 
        abort 0
    }
}

//# run Test1::M1::f1

// Location will show up as Test2::M1::f1 since the runtime module ID is resolved to the upgraded version
//# run Test2::M1::f1

// Location will show up as Test3::M1::f1 as the runtime module ID  is resolved to the upgraded version
//# run Test3::M1::f1
