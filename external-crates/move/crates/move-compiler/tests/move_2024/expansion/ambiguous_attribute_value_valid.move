#[allow(duplicate_alias)]
module a::m {
    use a::m;
    use std::vector;

    // unbound name, but bound address
    #[ext(attr = a)]
    fun t1() {}

    // unbound name, but bound module
    #[ext(attr = m)]
    fun t2() {}

    // unbound name in any case
    #[ext(attr = x)]
    fun t3() {}

    // Bit strange but we currently always resolve to the builtin
    #[ext(attr = vector)]
    fun t4() {}
}
