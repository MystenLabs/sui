// tests the validity of adding annotations on the entirety of a lambda

module 0x42::m;

macro fun do<$T, $R>($f: |$T| -> $R, $arg: $T): $R {
    $f($arg)
}

fun main() {
    do!((|x| { x }: |u8| -> u8), 42);
    do!(((|x| { x }: |bool| -> bool): |bool| -> bool), false);
    do!(
        ((|_| { vector[] }: |vector<vector<u8>>| -> vector<u8>): |vector<vector<u8>>| -> vector<u8>),
        vector[]
    );
}
