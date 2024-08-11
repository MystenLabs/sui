module 0x1::l {
    #[test]
    fun foo(a: u64) { 
        _ = a;
    }

    #[test, expected_failure]
    fun go(b: u64) { 
        _ = b;
    }

    #[test]
    fun qux(c: u64, d: bool) { 
        _ = c;
        _ = d;
    }

    #[test]
    fun qux_vec(c: vector<u8>) { 
        _ = c;
    }
}
