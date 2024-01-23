module a::m {
    macro fun apply1(f: || u64): u64 {
        f()
    }

    macro fun apply2(f: || u64): u64 {
        apply1!(f)
    }

    macro fun apply3(f: || u64): u64 {
        apply2!(|| apply2!(f))
    }

    fun t() {
        apply1!(|| 0);
        apply2!(|| 0);
        apply3!(|| 0);
    }
}
