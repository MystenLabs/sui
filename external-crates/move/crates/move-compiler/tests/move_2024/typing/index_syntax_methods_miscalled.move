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
    public fun borrow_s(s: &S, i: u64): &u64 {
        &s.t[i]
    }

    #[syntax(index)]
    public fun borrow_s_mut(s: &mut S, i: u64): &mut u64 {
        &mut s.t[i]
    }

    public fun miscall0(s: &S, i: u32): &u64 {
        &s.t[i]
    }

    public fun miscall1(s: &mut S, i: u32): &mut u64 {
        &mut s.t[i]
    }

    public fun miscall2<T>(s: &S, i: T): &u64 {
        &s.t[i]
    }


    public fun miscall3<T>(s: &mut S, i: T): &mut u64 {
        &mut s.t[i]
    }

}

module a::invalid {
    use a::s;

    fun miscall0(s: &s::S, i: u32): &u64 {
        &s[i]
    }

    fun miscall1(s: &mut s::S, i: u32): &mut u64 {
        &mut s[i]
    }

    fun miscall2<T>(s: &s::S, i: T): &u64 {
        &s[i]
    }

    fun miscall3<T>(s: &mut s::S, i: T): &mut u64 {
        &mut s[i]
    }

}

module a::mirror {

    public struct Q has drop {}

    #[syntax(index)]
    public fun borrow_mirror_mut(_q: &mut Q, i: &mut u64): &mut u64 { i }

    #[syntax(index)]
    public fun borrow_mirror(_q: &Q, i: &mut u64): &u64 { i }

    fun miscall0(q: &Q, i: u32): &u64 {
        &q[i]
    }

    fun miscall1(q: &mut Q, i: u32): &mut u64 {
        &mut q[i]
    }

    fun miscall2<T>(q: &Q, i: T): &u64 {
        &q[i]
    }

    fun miscall3<T>(q: &mut Q, i: T): &mut u64 {
        &mut q[i]
    }

}

module a::ambiguous {

    public fun miscall1<T>(v: &vector<T>, i: u32): &T {
        &v[i]
    }

    public fun miscall2<T>(v: &mut vector<T>, i: u32): &mut T {
        &mut v[i]
    }

    public fun miscall3<T,U>(v: &vector<T>, i: U): &T {
        &v[i]
    }

    public fun miscall4<T,U>(v: &mut vector<T>, i: U): &mut T {
        &mut v[i]
    }

}

module a::too_many_args {

    public fun miscall1<T>(v: &vector<T>, i: u64, j: u64): &T {
        &v[i, j]
    }

    public fun miscall2<T>(v: &mut vector<T>, i: u64, j: u64): &mut T {
        &mut v[i, j]
    }

    public fun miscall3<T,U,V>(v: &vector<T>, i: U, j: V): &T {
        &v[i, j]
    }

    public fun miscall4<T,U>(v: &mut vector<T>, i: U, j : V): &mut T {
        &mut v[i, j]
    }

}

module a::too_few_args {

    public struct S has drop { t: vector<u64> }

    #[syntax(index)]
    public fun borrow_s(s: &S, i: u64, j: u64): &u64 {
        &s.t[i + j]
    }

    #[syntax(index)]
    public fun borrow_s_mut(s: &mut S, i: u64, j: u64): &mut u64 {
        &mut s.t[i + j]
    }

    fun miscall0(s: &S, i: u64): &u64 {
        &s[i]
    }

    fun miscall1(s: &mut S, i: u64): &mut u64 {
        &mut s[i]
    }

}


