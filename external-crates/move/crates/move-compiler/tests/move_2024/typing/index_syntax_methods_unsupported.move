#[defines_primitive(vector)]
module std::vector {
    #[bytecode_instruction]
    native public fun empty<Element>(): vector<Element>;

    #[bytecode_instruction]
    #[syntax(index)]
    native public fun borrow<Element>(v: &vector<Element>, i: u64): &Element;

    #[bytecode_instruction]
    #[syntax(index)]
    native public fun borrow_mut<Element>(v: &mut vector<Element>, i: u64): &mut Element;
}

module a::s {

    public struct S has drop { t: vector<u64> }

    #[syntax(index)]
    public fun lookup_s(s: &S, i: u64): &u64 {
        &s.t[i]
    }

    #[syntax(index)]
    public fun lookup_s_mut(s: &mut S, i: u64): &mut u64 {
        &mut s.t[i]
    }

    #[syntax(for)]
    public fun for_s(s: &S, i: u64): &u64 {
        &s.t[i]
    }

    #[syntax(assign)]
    public fun assign_s(s: &mut S, i: u64, _value: u64): &mut u64 {
        &mut s.t[i]
    }

    #[syntax(nonsense)]
    public fun nonsense_s(s: &mut S, i: u64, _value: u64): &mut u64 {
        &mut s.t[i]
    }

}

