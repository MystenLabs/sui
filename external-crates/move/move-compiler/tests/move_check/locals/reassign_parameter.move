module 0x8675309::M {
    struct R {}

    public fun reassign_parameter(r: R, cond: bool) {
        let R { } = r;
        r = R {};
        if (cond) {
            let R { } = r;
        }
    }

}
