module a::invalid {

    public struct A<T> has drop { t: vector<T> }

    #[syntax(index)]
    public fun borrow_a<Q,T>(_s: &A<Q>, _j: T): &Q { abort 0 }

    #[syntax(index)]
    public fun borrow_mut_a<T,Q>(_s: &mut A<T>, _j: T): &mut T { abort 0 }

    public struct B<T> has drop { t: vector<T> }

    #[syntax(index)]
    public fun borrow_b<Q,T>(_s: &B<Q>, _j: Q): &T { abort 0 }

    #[syntax(index)]
    public fun borrow_mut_b<T,Q>(_s: &mut B<T>, _j: T): &mut T { abort 0 }

    public struct C<T> has drop { t: vector<T> }

    #[syntax(index)]
    public fun borrow_c<Q,T>(_s: &C<T>, _j: Q): &Q { abort 0 }

    #[syntax(index)]
    public fun borrow_mut_c<T,Q>(_s: &mut C<T>, _j: T): &mut T { abort 0 }

}
