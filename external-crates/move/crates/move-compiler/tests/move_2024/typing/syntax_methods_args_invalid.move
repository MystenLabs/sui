module a::invalid0 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_t<T>(s: &S<T>, i: u64): &u64 { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_mut<T>(s: &mut S<T>, i: T): &mut u64 { abort 0 }

}

module a::invalid1 {
    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_t<T,U,V>(s: &S<T>, i: U, j: V): &u64 { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_mut<Q,R>(s: &mut S<Q>, i: R, j: R): &mut u64 { abort 0 }

}

module a::invalid2 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_t<T>(s: &S<T>, i: u64, j: T): &T { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_mut<T>(s: &mut S<T>, i: u32, j: T): &mut T { abort 0 }

}

module a::invalid3 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_t<T,Q>(s: &S<T>, i: u64, j: Q): &T { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_mut<T>(s: &mut S<T>, i: u64, j: T): &mut T { abort 0 }

}

module a::invalid4 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_t<T,Q>(s: &S<T>, i: u64, j: Q): &T { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_mut<T,Q>(s: &mut S<T>, i: u32, j: T): &mut T { abort 0 }

}

module a::invalid5 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_t<T,R>(s: &S<T>, i: R, j: T): &T { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_mut<T,R>(s: &mut S<T>, i: T, j: T): &mut T { abort 0 }

}

module a::invalid6 {
    use std::vector;

    public struct S<T> has drop { t: vector<T> }

    #[syntax(index)]
    public fun lookup_t<T>(s: &S<T>, i: u64): &T {
        vector::borrow(&s.t, i)
    }

    #[syntax(index)]
    public fun lookup_mut<T>(s: &mut S<T>, i: u32): &mut T {
        vector::borrow_mut(&mut s.t, (i as u64))
    }

}
