module a::m {
    // meaningless to have mut _
    fun foo(mut _: u64) {
        let mut _ = 0;
        callf!(|mut _: u64| ());
    }

    macro fun callf($f: |u64|) {
        $f(0)
    }


}
