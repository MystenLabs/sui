address 0x42 {
module M {
    // Positional struct declarations are not supported till 2024
    struct Foo { f: u64 }

    fun f(x: Foo) {
        Foo(_) = x;
    }
}
}
