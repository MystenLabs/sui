module a::m {
    macro fun call($f: |u64| -> u64): u64 {
        $f(42)
    }

    fun t() {
        call!(|x| -> u64 'a: 0); // parsing error needs a block
    }

    fun t2() {
        call!(|x| -> u64 'a: loop { break 'a 0 }); // parsing error needs a block
    }

    fun t3() {
        call!(|x| -> u64 'a: { return 'a x } + 1); // parsing error, lambdas cant appear in binop
    }

    fun t4() {
        call!(|x| -> u64 { x } + 1); // parsing error, lambdas cant appear in binop
    }
}
