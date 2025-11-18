module 0x8675309::M {
    struct R {
        f: bool
    }

    fun t0(r: R) {
        !&true;
        !&false;
        !0u64;
        !1u64;
        !r;
        !r;
        !(0u64, false);
        !();
    }
}
