// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses A0=0x0 A1=0x0 B0=0x0 B1=0x0 --accounts A

//# publish --upgradeable --sender A
module A0::base {
    public enum Foo<phantom T> { V0 }
}

//# upgrade --package A0 --upgrade-capability 1,1 --sender A
module A1::base {
    public enum Foo<T: store> { V0 }
}

//# upgrade --package A0 --upgrade-capability 1,1 --sender A
module A1::base {
    public enum Foo<T: copy> { V0 }
}

//# upgrade --package A0 --upgrade-capability 1,1 --sender A
module A1::base {
    public enum Foo<T: drop> { V0 }
}

//# upgrade --package A0 --upgrade-capability 1,1 --sender A
module A1::base {
    public enum Foo<T: copy + drop + key> { V0 }
}
