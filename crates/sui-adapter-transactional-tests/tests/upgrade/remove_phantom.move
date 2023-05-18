// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses A0=0x0 A1=0x0 B0=0x0 B1=0x0 --accounts A

//# publish --upgradeable --sender A
module A0::base {
    struct Foo<phantom T> { }
}

//# upgrade --package A0 --upgrade-capability 1,1 --sender A
module A1::base {
    struct Foo<T> { }
}

//# upgrade --package A0 --upgrade-capability 1,1 --sender A
module A1::base {
    struct Foo<T: store> { }
}

