module a::invalid {

    public struct S<T> has drop { t: vector<T> }

    #[syntax(index)]
    public fun borrow<T,Q>(_s: &S<T>, _j: Q): &T { abort 0 }

    #[syntax(index)]
    public fun borrow_mut<T,Q>(_s: &mut S<T>, _j: T): &mut T { abort 0 }

}


