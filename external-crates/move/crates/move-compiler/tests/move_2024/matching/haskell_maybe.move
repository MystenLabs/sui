module 0x42::maybe {

    public enum Maybe<T> has drop {
        Just(T),
        Nothing
    }

    macro fun maybe<$A,$B: drop>($b: $B, $f: |$A| -> $B, $ma: Maybe<$A>): $B {
        match ($ma) {
            Maybe::Just(a) => $f(a),
            Maybe::Nothing => $b
        }
    }

    fun maybe_macro_call<A: drop>(ma: Maybe<A>, a: A): A {
        maybe!(a, |x| { x }, ma)
    }

    fun maybe_macro_call_2() {
        let m = maybe!(10, |x| { x }, Maybe::Just(5));
        assert!(m == 5, 1);
        let n = maybe!(10, |x| { x }, Maybe::Nothing);
        assert!(n == 10, 2);
    }

    fun is_just<A>(ma: &Maybe<A>): bool {
        match (ma) {
            Maybe::Just(_) => true,
            Maybe::Nothing => false,
        }
    }

    fun is_nothing<A>(ma: &Maybe<A>): bool {
        !is_just(ma)
    }

    fun from_just<A>(ma: Maybe<A>): A {
        match (ma) {
            Maybe::Just(a) => a,
            Maybe::Nothing => abort 0
        }
    }

    fun from_maybe<A: drop>(a: A, ma: Maybe<A>): A {
        match (ma) {
            Maybe::Just(a) => a,
            Maybe::Nothing => a
        }
    }

    fun push<T>(_v: &mut vector<T>, _t: T) { abort 0 }
    fun pop<T>(_v: &mut vector<T>): T { abort 0 }
    fun is_empty<T>(_v: &vector<T>): bool { abort 0 }
    fun reverse<T>(_v: vector<T>): vector<T> { abort 0 }

    fun cat_maybe<A: drop>(mut ls: vector<Maybe<A>>): vector<A> {
        let mut output = vector[];
        while (!is_empty(&ls)) {
            match (pop(&mut ls)) {
                Maybe::Just(a) => push(&mut output, a),
                Maybe::Nothing => (),
            }
        };
        reverse(output)
    }

    macro fun map_maybe<$A, $B>($f: |$A| -> Maybe<$B>, $ls: &mut vector<$A>): vector<$B> {
        let mut output = vector[];
        while (!is_empty($ls)) {
            match ($f(pop($ls))) {
                Maybe::Just(a) => push(&mut output, a),
                Maybe::Nothing => (),
            }
        };
        reverse(output)
    }

    fun macro_map_maybe_call(va: &mut vector<u64>): vector<u64> {
        map_maybe!(
            |a| { if (a % 2 == 0) { Maybe::Just(a) } else { Maybe::Nothing }},
            va
        )
    }

}
