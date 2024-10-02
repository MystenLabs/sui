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
        is_x!(0);
        is_x_ret!(0);
        is_num!(X());
        is_num!(@0);
        is_num!(vector[0]);
    }
}
