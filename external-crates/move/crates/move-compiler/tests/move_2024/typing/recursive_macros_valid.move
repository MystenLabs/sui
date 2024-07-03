module a::m {
    macro fun apply1($f: || -> u64): u64 {
        $f()
    }

    macro fun apply2($f: || -> u64): u64 {
        $f()
    }


    fun t() {
        // macros can call themselves, as long as the number of calls is finite/explicit
        apply1!(|| apply1!(|| 0));
        apply1!(|| apply1!(|| apply1!(|| 0)));
        apply1!(|| apply1!(|| apply1!(|| apply1!(|| 0))));
        apply2!(|| apply1!(|| apply1!(|| apply2!(|| 1))));
    }
}
