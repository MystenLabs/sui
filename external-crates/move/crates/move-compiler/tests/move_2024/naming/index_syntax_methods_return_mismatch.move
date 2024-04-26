#[defines_primitive(vector)]
module std::vector {
    #[bytecode_instruction]
    native public fun empty<Element>(): vector<Element>;

    #[bytecode_instruction]
    native public fun borrow<Element>(v: &vector<Element>, i: u64): &Element;

    #[bytecode_instruction]
    native public fun borrow_mut<Element>(v: &mut vector<Element>, i: u64): &mut Element;
}

// Incorrect usage

module a::invalid0 {



    public struct S has drop { t: vector<u64> }

    #[syntax(index)]
    public fun borrow_t(s: &S, i: u64): &mut u64 {
        vector::borrow(&s.t, i)
    }

    #[syntax(index)]
    public fun borrow_t_mut(s: &mut S, i: u64): &mut u64 {
        vector::borrow_mut(&mut s.t, i)
    }

}

module a::invalid1 {



    public struct S has drop { t: vector<u64> }

    #[syntax(index)]
    public fun borrow_t(s: &S, i: u64): &u64 {
        vector::borrow(&s.t, i)
    }

    #[syntax(index)]
    public fun borrow_t_mut(s: &mut S, i: u64): &u64 {
        vector::borrow_mut(&mut s.t, i)
    }

}

module a::invalid2 {


    public struct S has drop { t: vector<u64> , q: vector<u32> }

    #[syntax(index)]
    public fun borrow_t(s: &S, i: u64): &u64 {
        vector::borrow(&s.t, i)
    }

    #[syntax(index)]
    public fun borrow_q_mut(s: &mut S, i: u64): &mut u32 {
        vector::borrow_mut(&mut s.q, i)
    }

}

module a::invalid3 {


    public struct S has drop { t: vector<u64> }

    #[syntax(index)]
    public fun borrow_t(s: &S, i: u64): &u64 {
        vector::borrow(&s.t, i)
    }

    #[syntax(index)]
    public fun borrow_q_mut(s: &mut S, i: u64): u64 {
        *vector::borrow_mut(&mut s.t, i)
    }

}

module a::invalid4 {



    public struct S has drop { t: vector<u64> }

    #[syntax(index)]
    public fun borrow_t(s: &S, i: u64): u64 {
        *vector::borrow(&s.t, i)
    }

    #[syntax(index)]
    public fun borrow_t_mut(s: &mut S, i: u64): &mut u64 {
        vector::borrow_mut(&mut s.t, i)
    }

}
