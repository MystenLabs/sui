#[ext(custom_attr)]
address 0x42 {
#[ext(custom_attr)]
module M {
    #[ext(custom_attr)]
    use 0x42::N;

    #[ext(custom_attr)]
    struct S {}

    #[ext(custom_attr)]
    const C: u64 = 0;

    #[ext(custom_attr)]
    public fun foo() { N::bar() }
}
}

#[ext(custom_attr)]
module 0x42::N {
    #[ext(custom_attr)]
    friend 0x42::M;

    #[ext(custom_attr)]
    public fun bar() {}
}

#[ext(custom_attr)]
module 0x42::m {
    #[ext(custom_attr)]
    use 0x42::M;

    #[ext(custom_attr)]
    const C: u64 = 0;

    #[ext(custom_attr)]
    fun main() {
        M::foo();
    }
}
