// testing macros called without ! and vice versa
module a::m {
    public struct X()
    fun not_a_macro(_: X) {
    }
    macro fun a_macro(_: X) {}

    fun t() {
        not_a_macro!(X());
        a_macro(X());
        X().not_a_macro!();
        X().a_macro();
    }
}
