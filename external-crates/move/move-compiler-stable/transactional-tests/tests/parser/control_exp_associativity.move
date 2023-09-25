//# publish
module 0x42::M {
    fun t(): u64 {
        // 1 + (if (false) 0 else (10 + 10))
        let x = 1 + if (false) 0 else 10 + 10;
        assert!(x == 21, 0);
        // true && (if (false) false else (10 == 10))
        let x = true && if (false) false else 10 == 10;
        assert!(x, 0);
        // (if (false) 0 else 10 ) == 10
        let x = if (false) 0 else { 10 } == 10;
        assert!(x, 0);
        // (if (true) 0 else 10) + 1
        let x = if (true) 0 else { 10 } + 1;
        assert!(x == 1, 0);
        // if (true) 0 else (10 + 1)
        let x = if (true) 0 else ({ 10 }) + 1;
        assert!(x == 0, 0);
        42
    }

}

//# run 0x42::M::t
