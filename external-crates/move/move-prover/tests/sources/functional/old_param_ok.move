module 0x2::Token {
    struct Token<phantom T> has store { value: u64 }

    fun withdraw<T>(token: &mut Token<T>, value: u64): Token<T> {
        assert!(token.value >= value, 42);
        token.value = token.value - value;
        Token { value }
    }

    public fun split<T>(token: Token<T>, value: u64): (Token<T>, Token<T>) {
        let other = withdraw(&mut token, value);
        (token, other)
    }
    spec split {
        aborts_if token.value < value;
        ensures token.value == result_1.value + result_2.value;
    }
}
