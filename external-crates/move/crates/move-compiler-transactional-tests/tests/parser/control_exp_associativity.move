//# publish
module 0x42::M {
    fun t(): u64 {
        // 1 + (if (false) 0 else (10 + 10))
        let x = 1 + if (false) 0 else 10 + 10u64;
        assert!(x == 21, 0);
        // true && (if (false) false else (10 == 10))
        let x = true && if (false) false else 10 == 10u64;
        assert!(x, 0);
        // (if (false) 0 else 10 ) == 10
        let x = if (false) 0 else { 10u64 } == 10;
        assert!(x, 0);
        // (if (true) 0 else 10) + 1
        let x = if (true) 0 else { 10 } + 1;
        assert!(x == 1u64, 0);
        // if (true) 0 else (10 + 1)
        let x = if (true) 0 else ({ 10 }) + 1;
        assert!(x == 0u64, 0);
        42
    }

}

//# run 0x42::M::t
