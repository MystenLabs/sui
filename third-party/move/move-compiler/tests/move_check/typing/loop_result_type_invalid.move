address 0x2 {

module X {
    struct R {}
}

module M {
    use 0x2::X;

    fun t0(): X::R {
        loop { if (false) break }
    }

    fun t1(): u64 {
        loop { let _x = 0; break }
    }

    fun t2() {
        foo(loop { break })
    }

    fun foo(_: u64) {}

    fun t3() {
        let _x = loop { break };
        let (_x, _y) = loop { if (false) break };
    }
}

}
