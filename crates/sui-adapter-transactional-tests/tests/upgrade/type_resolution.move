// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses A0=0x0 A1=0x0 A2=0x0 --accounts A --protocol-version 86

//# publish --upgradeable --sender A
module A0::m {
    public struct A {}
}

//# upgrade --package A0 --upgrade-capability 1,1 --sender A
module A1::m {
    public struct A {}
    public struct B {}
}

//# upgrade --package A1 --upgrade-capability 1,1 --sender A
module A2::m {
    public struct A {}
    public struct B {}
    public struct C {}
    entry fun call<T>() { }
}

// Resolves fine
//# run A2::m::call --type-args A0::m::A --sender A

// Resolves fine
//# run A2::m::call --type-args A1::m::A --sender A

// Resolves fine
//# run A2::m::call --type-args A2::m::A --sender A

// Fails to resolve
//# run A2::m::call --type-args 0x0::m::A --sender A


// Resolves fine
//# run A2::m::call --type-args A0::m::B --sender A

// Resolves fine
//# run A2::m::call --type-args A1::m::B --sender A

// Resolves fine
//# run A2::m::call --type-args A2::m::B --sender A

// Fails to resolve
//# run A2::m::call --type-args 0x0::m::B --sender A


// Resolves fine
//# run A2::m::call --type-args A0::m::C --sender A

// Resolves fine
//# run A2::m::call --type-args A1::m::C --sender A

// Resolves fine
//# run A2::m::call --type-args A2::m::C --sender A

// Fails to resolve
//# run A2::m::call --type-args 0x0::m::C --sender A
