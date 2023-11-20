module 0x42::used_fun_tparam {
    // no warnings related to unused function type params should be generated for functions in this
    // module


    struct S<phantom T: key + drop> has key, drop {
    }

    public fun foo<T>(): T {
        abort 0
    }

    public fun bar<T>(_: T) {
        abort 0
    }


    public fun no_warn_sig_direct<T>(v: T): T {
        v
    }

    public fun no_warn_sig_indirect<T: key + drop>(v: S<T>): S<T> {
        v
    }

    public fun no_warn_pack<T: key + drop>() {
        let _ = S<T> {};
    }

    public fun no_warn_pack_unpack<T: key + drop>() {
        let x = S {};
        let S<T> {} = x;
    }

    public fun no_warn_bind<T: key + drop>() {
        let _: T = foo();
    }

    public fun no_warn_call<T: key + drop>() {
        let _ = foo<T>();
    }

    public fun no_warn_annotation<T: key + drop>() {
        let x = foo();
        bar((x: T));
    }




}
