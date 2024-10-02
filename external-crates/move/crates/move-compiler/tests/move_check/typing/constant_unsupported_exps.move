address 0x42 {
module X {
    public fun f_public() {}
    public(friend) fun f_friend() {}
    fun f_private() {}
}

module M {
    struct R has key {}
    struct B has drop { f: u64 }

    const FLAG: bool = false;
    const C: u64 = {
        let x = 0;
        let s: signer = abort 0;
        let b = B { f: 0 };
        &x;
        &mut x;
        f_public();
        f_script();
        f_friend();
        f_private();
        0x42::X::f_public();
        0x42::X::f_script();
        0x42::X::f_friend();
        0x42::X::f_private();
        freeze(&mut x);
        assert!(true, 42);
        if (true) 0 else 1;
        loop ();
        loop { break; continue; };
        while (true) ();
        x = 1;
        return 0;
        abort 0;
        *(&mut 0) = 0;
        b.f = 0;
        b.f;
        *&b.f;
        FLAG;
        0
    };
    public fun f_public() {}
    public(friend) fun f_friend() {}
    fun f_private() {}
}
}
