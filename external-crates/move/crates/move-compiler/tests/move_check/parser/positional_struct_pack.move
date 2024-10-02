address 0x42 {
module M {
    // Positional struct declarations are not supported till 2024
    struct Foo has drop { x: u64 }

    fun f() {
        let _ = Foo(0);
    }
}
}

