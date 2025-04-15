// tests invalid annotations on the entirety of a lambda

module 0x42::m;

macro fun do<$T, $R>($f: |$T| -> $R, $arg: $T): $R {
    $f($arg)
}

macro fun do2<$T, $R>($f: |$T, $T| -> $R, $arg: $T): $R {
    $f($arg, $arg)
}


fun bad_types() {
    do!((|x| { x }: |u8| -> u16), 42);
    do!(((|x| { x }: |bool| -> u8): |u8| -> bool), false);
}

fun non_lambda() {
    do!((|x| { x }: u8), 42);
    do!(((|x| { x }: bool): |bool| -> bool), false);
}

fun bad_arity() {
    do!((|x| { x }: |u8, u8| -> u8), 42);
    do2!((|x, y| { x + y }: |u8| -> u8), 42);

    do!(((|_| { false }: |u8| -> bool): |u8, u8| -> bool), 0);
    do!(((|_| { false }: |u8, u8| -> bool): |u8| -> bool), 0);

    do2!(((|_, _| { false }: |u8| -> bool): |u8, u8| -> bool), 0);
    do2!(((|_, _| { false }: |u8, u8| -> bool): |u8| -> bool), 0);
}
