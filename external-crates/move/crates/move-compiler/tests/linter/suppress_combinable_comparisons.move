#[allow(lint(combinable_comparisons))]
module a::m {
    fun t(x: u64, y: u64): bool {
        x > y || x == y
    }
}


module a::n {
    #[allow(lint(combinable_comparisons))]
    const C: bool = 5 > 3 || 5 == 3;

    #[allow(lint(combinable_comparisons))]
    fun t(x: u64, y: u64): bool {
        x > y || x == y
    }
}
