module a::m {
    #[allow(untyped_literal)]
    fun foo() {
        let x = 0;
        while (x < 10) {
            x = x + 1;
        }
    }
}
