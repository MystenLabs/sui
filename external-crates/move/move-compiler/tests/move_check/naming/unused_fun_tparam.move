module 0x42::unused_fun_tparam {

    public fun unused<T>(): u64 {
        42
    }

    public fun one_unused<T1, T2>(v: T1): T1 {
        v
    }

    public fun all_unused<T1, T2>(): u64 {
        42
    }

}
