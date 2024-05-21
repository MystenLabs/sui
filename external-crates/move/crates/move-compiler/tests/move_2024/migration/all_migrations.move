module a::m {

    #[test_only]
    friend a::b;

    friend a::c;

    #[ext(
        some_thing
        )
    ]
    friend a::d;

    #[ext(
        q =
            10,
        b
        )
    ]
    friend a::e;

    struct S { f: u64 }

    struct LongerName {
        f: u64,
        x: S,
    }

    struct Positional(u64, u64, u64)

    fun t0(x: u64, s: S): u64 {
        let S { f: fin } = s;
        fin = 10;
        x = 20;
        fin + x
    }

    public(friend) fun t1() {}

    public(
        friend) fun t2() {}

    public(
        friend
        ) fun t3() {}

    public(
        friend
    ) fun t4() {}
}

module a::b {}
module a::c {}
module a::d {}
module a::e {}
