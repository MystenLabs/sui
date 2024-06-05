module 0x1::l {
    #[rand_test]
    fun foo(a: u64) { 
        _ = a;
    }

    #[rand_test]
    fun go(b: u64) { 
        _ = b;
    }

    #[rand_test]
    fun qux(c: u64, d: bool) { 
        _ = c;
        _ = d;
    }

    #[rand_test]
    fun qux_vec(c: vector<u8>) { 
        _ = c;
    }
}
