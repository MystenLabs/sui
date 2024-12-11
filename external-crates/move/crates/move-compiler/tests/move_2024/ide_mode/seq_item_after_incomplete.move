// tests if a sequence item  is valid despite
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

    public fun foo(num: u64):u64 {
        num
    }

    public fun test6(param: u64): u64 {
        let _v         // parses correctly but without semicolon
        foo(param)     // returned value should still be correct if foo invocation parses correctly
    }


    public struct SomeStruct has drop, copy {}

    public fun bar(param: SomeStruct): SomeStruct {
        param
    }

    public fun test7(param: SomeStruct): SomeStruct {
        param.bar       // parses correctly
        param.bar()     // returned value should still be correct if foo invocation parses correctly
    }

}
