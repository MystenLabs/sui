module 0x1::l {
    #[random_test]
    fun foo(a: u64) { 
        _ = a;
    }

    #[random_test]
    fun go(b: u64) { 
        _ = b;
    }

    #[random_test]
    fun qux(c: u64, d: bool) { 
        _ = c;
        _ = d;
    }

    #[random_test]
    fun qux_vec(c: vector<u8>) { 
        _ = c;
    }
}
