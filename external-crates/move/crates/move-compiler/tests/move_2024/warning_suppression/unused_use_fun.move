#[allow(unused_use)]
module a::m {
    use fun foo as X.f;
    public struct X {}
    public fun foo(_: &X) {}
}

#[allow(unused_use)]
module 1::m {
    use fun a::m::foo as a::m::X.f;
    fun main() {}
}

module a::m2 {
    use a::m::{X, foo};
    #[allow(unused_use)]
    public fun bar(_: &X) {
        use fun foo as X.f;
    }
    #[allow(unused_use)]
    const C: u64 = {
        use fun foo as X.f;
        0
    };
}

#[allow(unused)]
module 2::m {
    use fun a::m::foo as a::m::X.f;
    fun main2() {}
}

module a::m3 {
    use a::m::{X, foo};
    #[allow(all)]
    public fun bar(_: &X) {
        use fun foo as X.f;
    }
    #[allow(all)]
    const C: u64 = {
        use fun foo as X.f;
        0
    };
}
