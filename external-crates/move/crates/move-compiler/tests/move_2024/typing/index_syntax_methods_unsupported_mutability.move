#[defines_primitive(vector)]
module std::vector {
    #[bytecode_instruction]
    native public fun empty<Element>(): vector<Element>;

    #[bytecode_instruction]
    native public fun borrow<Element>(v: &vector<Element>, i: u64): &Element;

    #[bytecode_instruction]
    native public fun borrow_mut<Element>(v: &mut vector<Element>, i: u64): &mut Element;
}

// Correct usage
module a::m {



    public struct S has drop { t: vector<u64> }

    #[syntax(index)]
    public fun borrow(s: &S, i: u64): &u64 {
        vector::borrow(&s.t, i)
    }

    public struct T has drop { t: vector<u64> }

    #[syntax(index)]
    public fun borrow_mut(t: &mut T, i: u64): &mut u64 {
        vector::borrow_mut(&mut t.t, i)
    }

    public fun deref(x: &u64): u64 { *x }
    public fun mut_deref(x: &mut u64): u64 { *x }

    public fun main() {
        use fun deref as u64.deref;
        use fun mut_deref as u64.mut_deref;

        let mut s = S { t: vector[] };
        let t = T { t: vector[] };
        let i = 0;
        s[i].mut_deref();
        t[i].deref();
    }
}
