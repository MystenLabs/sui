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

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_t<T>(s: &S<T>, i: T): &T { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_mut<T>(s: &mut S<T>, i: T): &mut T { abort 0 }

}

module a::valid1 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_t<T>(s: &S<T>, i: T): &T { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_mut<U>(s: &mut S<U>, i: U): &mut U { abort 0 }

}

module a::valid2 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_t<T,Q>(s: &S<T>, i: Q): &T { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_mut<T,R>(s: &mut S<T>, i: R): &mut T { abort 0 }

}

module a::valid3 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_t<T,Q>(s: &S<T>, i: Q): &T { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_mut<A,B>(s: &mut S<A>, i: B): &mut A { abort 0 }

}

module a::valid4 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_t<T,U,V>(s: &S<T>, i: U, j: V): &u64 { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_mut<A,B,C>(s: &mut S<A>, i: B, j: C): &mut u64 { abort 0 }

}

