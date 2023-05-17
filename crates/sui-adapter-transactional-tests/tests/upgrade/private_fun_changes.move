// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses V0=0x0 V1=0x0 V2=0x0 V3=0x0 V4=0x0 V5=0x0 V6=0x0 V7=0x0 V8=0x0 --accounts A

//# publish --upgradeable --sender A
module V0::base {
    fun f() { }
}

//# upgrade --package V0 --upgrade-capability 1,1 --sender A
module V1::base {
    fun f(_x: u64) { }
}

//# upgrade --package V1 --upgrade-capability 1,1 --sender A
module V2::base {
    entry fun f(): u64 { 0 }
}

//# upgrade --package V2 --upgrade-capability 1,1 --sender A
module V3::base {
    entry fun f(_f: u64): u64 { 0 }
}

//# upgrade --package V3 --upgrade-capability 1,1 --sender A
module V4::base {
    fun f(_f: u64): u64 { 0 }
}

//# upgrade --package V4 --upgrade-capability 1,1 --sender A
module V5::base {
}

//# upgrade --package V5 --upgrade-capability 1,1 --sender A
module V6::base {
    fun f(): u64 { 0 }
}

//# upgrade --package V6 --upgrade-capability 1,1 --sender A
module V7::base {
    public fun f(_f: u64): u64 { 0 }
}

//# upgrade --package V7 --upgrade-capability 1,1 --sender A
module V8::base {
    public fun f(): u64 { 0 }
}
