// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests that pure arguments have their types fixed/changed after being used by a mutable reference

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    use std::string::String;
    use std::ascii;

    public fun fix<T>(_: &mut T) {}

    public fun addr(_: address) {}
    public fun id(_: ID) {}

    public fun ascii_(_: ascii::String) {}
    public fun string(_: String) {}

    public fun vec<T: drop>(_: vector<T>) {}
    public fun opt<T: drop>(_: Option<T>) {}


}

//# programmable --inputs "hello"

//> 0: test::m1::ascii_(Input(0));
//> 1: test::m1::string(Input(0));
//> 2: test::m1::fix<std::ascii::String>(Input(0));
// now will fail as Input(0) if always a String
//> 3: test::m1::string(Input(0));

//# programmable --inputs @A

//> 0: test::m1::addr(Input(0));
//> 1: test::m1::id(Input(0));
//> 2: test::m1::fix<sui::object::ID>(Input(0));
// now will fail as Input(0) if always an ID
//> 3: test::m1::addr(Input(0));

//# programmable --inputs vector[0u64]

//> 0: test::m1::vec<u64>(Input(0));
//> 1: test::m1::opt<u64>(Input(0));
//> 2: test::m1::fix<vector<u64>>(Input(0));
// now will fail as Input(0) if always a vector
//> 3: test::m1::opt<u64>(Input(0));

//# programmable --inputs vector[]

//> 0: test::m1::vec<u64>(Input(0));
//> 1: test::m1::opt<u64>(Input(0));
//> 2: test::m1::fix<std::option::Option<u64>>(Input(0));
// now will fail as Input(0) if always an Option
//> 3: test::m1::vec<u64>(Input(0));
