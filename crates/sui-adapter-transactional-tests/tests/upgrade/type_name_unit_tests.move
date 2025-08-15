// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses A0=0x0 A1=0x0 A2=0x0 --accounts A

//# publish --upgradeable --sender A
module A0::m {
    public struct A {}

    public enum EA { V }

    public struct ACup<phantom T> {}
}

//# upgrade --package A0 --upgrade-capability 1,1 --sender A
module A1::m {
    public struct A {}
    public struct B {}

    public enum EA { V }
    public enum EB { V }

    public struct ACup<phantom T> {}
    public struct BCup<phantom T> {}
}

//# set-address A1 object(2,0)

//# upgrade --package A1 --upgrade-capability 1,1 --sender A
module A2::m {
    use std::type_name;

    public struct A {}
    public struct B {}

    public enum EA { V }
    public enum EB { V }

    public struct ACup<phantom T> {}
    public struct BCup<phantom T> {}

    fun dt_name(str: vector<u8>): vector<u8> {
        // skip: addr + "::" + "m" + "::"
        // take: until "<"
        let lt = std::ascii::string(b"<").pop_char().byte();
        str
            .skip(sui::address::length() * 2 + 2 + 1 + 2)
            .take_while!(|c| c != &lt)
    }

    // Checks the following in both `with_original_ids` and `with_defining_ids`
    // - That the address matches the case specified
    // - That the module name is `"m"`
    // - That the datatype name matches the `$name` specified
    macro fun case<$T>($original: address, $defining: address, $name: vector<u8>) {
        let original_address = $original;
        let defining_address = $defining;
        let name = $name;
        let original_string = original_address.to_ascii_string();
        let defining_string = defining_address.to_ascii_string();
        let og = type_name::with_original_ids<$T>();
        let def = type_name::with_defining_ids<$T>();
        assert!(og.address_string() == original_string, 0);
        assert!(og.module_string().into_bytes() == b"m", 1);
        assert!(dt_name(og.into_string().into_bytes()) == &name, 2);

        assert!(def.address_string() == defining_string, 3);
        assert!(def.module_string().into_bytes() == b"m", 4);
        assert!(dt_name(def.into_string().into_bytes()) == &name, 5);
    }

    // peels the inner typename, assuming the pattern  of "tn<inner>", returning "inner"
    fun inner_tn(str: vector<u8>): vector<u8> {
        let lt = std::ascii::string(b"<").pop_char().byte();
        let gt = std::ascii::string(b">").pop_char().byte();
        // sip: until "<"
        let mut str = str.skip_while!(|c| c != &lt);
        // remove "<"
        str.reverse();
        let c = str.pop_back();
        str.reverse();
        assert!(c == lt, lt as u64);
        // remove ">"
        let c = str.pop_back();
        assert!(c == gt, gt as u64);
        str
    }

    // peels the leading address and parses it using `sui::address::from_ascii_bytes`
    fun leading_address(str: vector<u8>): address {
        sui::address::from_ascii_bytes(&str.take(sui::address::length() * 2))
    }

    // Tests the following in both `with_original_ids` and `with_defining_ids`:
    // - That the properties of macro `case` are satisfied
    // - That the inner type name is correct
    // - That the inner type name has the correct address
    macro fun gen_case<$T, $Inner>(
        $cup_og: address,
        $cup_def: address,
        $name: vector<u8>,
        $inner_og: address,
        $inner_def: address,
    ) {
        let cup_og_address = $cup_og;
        let cup_def_address = $cup_def;
        let name = $name;
        let inner_og_address = $inner_og;
        let inner_def_address = $inner_def;
        case!<$T>(cup_og_address, cup_def_address, name);

        let cup_og = type_name::with_original_ids<$T>();
        let inner_og = type_name::with_original_ids<$Inner>();
        let inner_s = inner_tn(cup_og.into_string().into_bytes());
        assert!(&inner_s == inner_og.into_string().into_bytes(), 200);
        assert!(leading_address(inner_s) == inner_og_address, 201);


        let cup_def = type_name::with_defining_ids<$T>();
        let inner_def = type_name::with_defining_ids<$Inner>();
        let inner_s = inner_tn(cup_def.into_string().into_bytes());
        assert!(&inner_s == inner_def.into_string().into_bytes(), 202);
        assert!(leading_address(inner_s) == inner_def_address, 203);
    }

    // tests that type name is behaving correctly with regards to defining and original IDs for
    // non generic types
    entry fun cases(a0: address, a1: address) {
        assert!(a0 != @0, 100);
        assert!(a1 != @0, 101);

        case!<A>(a0, a0, b"A");
        case!<EA>(a0, a0, b"EA");

        case!<B>(a0, a1, b"B");
        case!<EB>(a0, a1, b"EB");
    }

    // tests that type name is behaving correctly with regards to defining and original IDs for
    // generic types
    entry fun generic_cases(a0: address, a1: address) {
        assert!(a0 != @0, 300);
        assert!(a1 != @0, 301);

        // outer defining a0, inner defining a0
        gen_case!<ACup<A>, A>(a0, a0, b"ACup", a0, a0);
        gen_case!<ACup<EA>, EA>(a0, a0, b"ACup", a0, a0);

        // outer defining a0, inner defining a1
        gen_case!<ACup<B>, B>(a0, a0, b"ACup", a0, a1);
        gen_case!<ACup<EB>, EB>(a0, a0, b"ACup", a0, a1);

        // outer defining a1, inner defining a0
        gen_case!<BCup<A>, A>(a0, a1, b"BCup", a0, a0);
        gen_case!<BCup<EA>, EA>(a0, a1, b"BCup", a0, a0);

        // outer defining a1, inner defining a1
        gen_case!<BCup<B>, B>(a0, a1, b"BCup", a0, a1);
        gen_case!<BCup<EB>, EB>(a0, a1, b"BCup", a0, a1);
    }
}

//# run A2::m::cases --args @A0 @A1

//# run A2::m::generic_cases --args @A0 @A1
