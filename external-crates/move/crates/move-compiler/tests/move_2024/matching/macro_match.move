module 0x42::maybe {

    public enum Maybe<T> {
        Just(T),
        Nothing
    }

    macro fun maybe<$A,$B: drop>($b: $B, $f: |$A| -> $B, $ma: Maybe<$A>): $B {
        match ($ma) {
            Maybe::Just(a) => $f(a),
            Maybe::Nothing => $b
        }
    }

    fun maybe_macro_call_2() {
        let m = maybe!(10, |x| { x }, Maybe::Just(5));
        assert!(m == 5, 1);
        let n = maybe!(10, |x| { x }, Maybe::Nothing);
        assert!(n == 10, 2);
    }

}
