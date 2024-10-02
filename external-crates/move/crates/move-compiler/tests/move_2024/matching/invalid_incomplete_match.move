module 0x42::m {

    public enum Tuple<A,B> {
        Two(A,B),
    }

    public enum Maybe<A> {
        Just(A),
        Nothing
    }

    fun match_invalid(x: &Tuple<Maybe<bool>, Maybe<u64>>): u64 {
        match (x) {
            Tuple::Two(Maybe::Just(true), Maybe::Just(5)) => 10,
        }
    }

}


