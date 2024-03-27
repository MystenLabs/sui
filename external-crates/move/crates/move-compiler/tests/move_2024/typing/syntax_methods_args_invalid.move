module a::invalid0 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow<T>(s: &S<T>, i: u64): &u64 { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow_mut<T>(s: &mut S<T>, i: T): &mut u64 { abort 0 }

}

module a::invalid1 {
    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow<T,U,V>(s: &S<T>, i: U, j: V): &u64 { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow_mut<Q,R>(s: &mut S<Q>, i: R, j: R): &mut u64 { abort 0 }

}

module a::invalid2 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow<T>(s: &S<T>, i: u64, j: T): &T { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow_mut<T>(s: &mut S<T>, i: u32, j: T): &mut T { abort 0 }

}

module a::invalid3 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow<T,Q>(s: &S<T>, i: u64, j: Q): &T { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow_mut<T>(s: &mut S<T>, i: u64, j: T): &mut T { abort 0 }

}

module a::invalid4 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow<T,Q>(s: &S<T>, i: u64, j: Q): &T { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow_mut<T,Q>(s: &mut S<T>, i: u32, j: T): &mut T { abort 0 }

}

module a::invalid5 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow<T,R>(s: &S<T>, i: R, j: T): &T { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow_mut<T,R>(s: &mut S<T>, i: T, j: T): &mut T { abort 0 }

}

module a::invalid6 {


    public struct S<T> has drop { t: vector<T> }

    #[syntax(index)]
    public fun borrow<T>(s: &S<T>, i: u64): &T {
        vector::borrow(&s.t, i)
    }

    #[syntax(index)]
    public fun borrow_mut<T>(s: &mut S<T>, i: u32): &mut T {
        vector::borrow_mut(&mut s.t, (i as u64))
    }

}

module a::invalid7 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow<T>(s: &S<T>, i: &u64, j: T): &T { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow_mut<T>(s: &mut S<T>, i: &mut u64, j: T): &mut T { abort 0 }

}

module a::invalid8 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow<T>(s: &S<T>, i: &u64, j: &T): &T { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow_mut<T>(s: &mut S<T>, i: &u64, j: &mut T): &mut T { abort 0 }

}

module a::invalid9 {

    public struct S<T> has drop { t: vector<T> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow<T>(s: &S<T>, i: &u64, j: &mut T): &T { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun borrow_mut<T>(s: &mut S<T>, i: &u64, j: &T): &mut T { abort 0 }

}
