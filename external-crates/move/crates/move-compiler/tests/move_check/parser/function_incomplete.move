// correct compilation of partial function definitions as evidenced by the ability to call all its
// variants without an error
module 0x42::m {
    fun just_name

    fun just_type_args<T>

    fun just_param<T>(_u: u64)

    fun just_ret<T>(_u: u64): u64

    fun everything<T>(u: u64): u64 {
        u
    }

    fun foo() {
        just_name();
        just_type_args<u64>();
        just_param<u64>(42);
        let _n1 = just_ret<u64>(42);
        let _n2 = everything<u64>(42);
    }
}
