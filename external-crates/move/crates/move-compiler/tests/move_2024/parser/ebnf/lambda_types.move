// Test: Lambda expressions with return types
// EBNF: LambdaExp, LambdaBindList, LambdaBinding
module 0x42::lambda_types;

macro fun apply<$T, $R>($f: |$T| -> $R, $x: $T): $R {
    $f($x)
}

macro fun apply2<$T, $U, $R>($f: |$T, $U| -> $R, $x: $T, $y: $U): $R {
    $f($x, $y)
}

macro fun thunk<$T>($f: || -> $T): $T {
    $f()
}

fun test_lambdas() {
    let _: u64 = apply!(|x| -> u64 { x + 1 }, 5);
    let _: u64 = apply!(|x: u64| -> u64 { x * 2 }, 10);
    let _: u64 = apply2!(|x, y| -> u64 { x + y }, 1, 2);
    let _: u64 = thunk!(|| -> u64 { 42 });
}

macro fun foreach<$T>($v: &vector<$T>, $f: |&$T|) {
    let v = $v;
    let mut i = 0;
    let n = v.length();
    while (i < n) {
        $f(&v[i]);
        i = i + 1;
    }
}

fun test_foreach() {
    let v = vector[1u64, 2, 3];
    let mut sum = 0u64;
    foreach!(&v, |x| { sum = sum + *x; });
}
