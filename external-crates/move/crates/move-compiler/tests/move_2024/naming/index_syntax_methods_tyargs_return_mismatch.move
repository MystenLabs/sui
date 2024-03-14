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

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow_t<T>(s: &S<T>, i: u64, j: T): &mut T { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow_mut<T>(s: &mut S<T>, i: u64, j: T): &mut T { abort 0 }

}

module a::invalid1 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow_t<T>(s: &S<T>, i: u64, j: T): &T { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow_mut<T>(s: &mut S<T>, i: u64, j: T): &T { abort 0 }

}

module a::invalid2 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow_t<T>(s: &S<T>, i: u64, j: T): &mut T { abort 0 }

}

module a::invalid3 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow_t<T>(s: &S<T>, i: u64, j: T): &mut T { abort 0 }

}

