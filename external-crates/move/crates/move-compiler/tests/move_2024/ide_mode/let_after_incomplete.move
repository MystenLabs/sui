// tests if a variable declaration is valid despite
// parsing error preceding it in various scenarios
module a::m {

    public fun test1(param: u64): u64 {
        let              //  does not parse correctly
        let _tmp1 = 42;
        _tmp1 + param
    }

    public fun test2(param: u64): u64 {
        let _v           // parses correctly but without semicolon
        let _tmp1 = 42;
        _tmp1 + param
    }

    public fun test3(param: u64): u64 {
        let _v =         // does not parse correctly
        let _tmp1 = 42;
        _tmp1 + param
    }

    public fun test4(param: u64): u64 {
        f                // parses correctly but without semicolon
        let _tmp1 = 42;
        _tmp1 + param
    }

    public fun test5(mut param: u64): u64 {
        param =         // does not parse correctly
        let _tmp1 = 42;
        _tmp1 + param
    }

}
