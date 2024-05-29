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
module a::valid0 {



    public struct S has drop { t: vector<u64> }

    #[syntax(index)]
    public fun borrow(s: &S, i: u64): &u64 {
        vector::borrow(&s.t, i)
    }

    #[syntax(index)]
    public fun borrow_mut(s: &mut S, i: u64): &mut u64 {
        vector::borrow_mut(&mut s.t, i)
    }

}

module a::valid1 {


    public struct S<T> has drop { t: vector<T> }

    #[syntax(index)]
    public fun borrow<T>(s: &S<T>, i: u64): &T {
        vector::borrow(&s.t, i)
    }

    #[syntax(index)]
    public fun borrow_mut<T>(s: &mut S<T>, i: u64): &mut T {
        vector::borrow_mut(&mut s.t, i)
    }

}

module a::valid2 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow<T>(s: &S<T>, i: u64, j: u64): &T { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow_mut<T>(s: &mut S<T>, i: u64, j: u64): &mut T { abort 0 }

}
