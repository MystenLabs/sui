module a::m {
    public struct X() has copy, drop;

    macro fun is_x<$T>($x: $T) {
        ($x: X);
    }

    macro fun is_x_ret<$T>($x: $T): X {
        $x
    }

    macro fun is_num<$T>($x: $T) {
        ($x as $T);
    }

    fun t() {
        is_x!(X());
        is_x_ret!(X());
        is_num!(0u8);
        is_num!(0u16);
        is_num!(0u32);
        is_num!(0u64);
        is_num!(0u128);
        is_num!(0u256);
    }
}
