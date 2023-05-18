// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses A0=0x0 A1=0x0 A2=0x0 A3=0x0 A4=0x0 --accounts A

//# publish --upgradeable --sender A
module A0::base {
    public fun f<T: store + copy + key>() { }
}

//# upgrade --package A0 --upgrade-capability 1,1 --sender A
module A1::base {
    public fun f<T: store + copy>() { }
}

//# upgrade --package A1 --upgrade-capability 1,1 --sender A
module A2::base {
    public fun f<T: store>() { }
}

//# upgrade --package A2 --upgrade-capability 1,1 --sender A
module A3::base {
    public fun f<T>() { }
}

//# upgrade --package A3 --upgrade-capability 1,1 --sender A
module A4::base {
    public fun f<T: store>() { }
}
