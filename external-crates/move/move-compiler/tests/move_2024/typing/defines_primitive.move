module a::m {
    public struct S {}
    public fun test(s: &signer) {
        let _: &address = s.as_address();
        let _: address = s.to_address();
        let _: u64 = 0u64.max(1);

        let mut v = vector[];
        let _: u64 = v.length();
        v.push_back(S {});
        let _: &S = v.borrow(0);
        let _: &mut S = v.borrow_mut(0);
        v.swap(0, 0);
        let S {} = v.pop_back();
        v.destroy_empty();
    }
}

#[defines_primitive(signer)]
module std::signer {
    native public fun borrow_address(s: &signer): &address;
    public fun as_address(s: &signer): &address {
        borrow_address(s)
    }
    public fun to_address(s: &signer): address {
        *borrow_address(s)
    }
}

#[defines_primitive(u64)]
module std::u64 {
    public fun max(a: u64, b: u64): u64 {
        if (a >= b) a
        else b
    }
}

#[defines_primitive(vector)]
module std::vector {
    #[bytecode_instruction]
    native public fun empty<Element>(): vector<Element>;

    #[bytecode_instruction]
    native public fun length<Element>(v: &vector<Element>): u64;

    #[bytecode_instruction]
    native public fun borrow<Element>(v: &vector<Element>, i: u64): &Element;

    #[bytecode_instruction]
    native public fun push_back<Element>(v: &mut vector<Element>, e: Element);

    #[bytecode_instruction]
    native public fun borrow_mut<Element>(v: &mut vector<Element>, i: u64): &mut Element;

    #[bytecode_instruction]
    native public fun pop_back<Element>(v: &mut vector<Element>): Element;

    #[bytecode_instruction]
    native public fun destroy_empty<Element>(v: vector<Element>);

    #[bytecode_instruction]
    native public fun swap<Element>(v: &mut vector<Element>, i: u64, j: u64);
}
