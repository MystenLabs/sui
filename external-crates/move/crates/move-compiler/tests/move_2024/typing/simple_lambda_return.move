module a::m;

macro fun call<$T>($f: || -> $T): $T {
    $f()
}

#[allow(dead_code)]
fun commands(cond: bool) {
    call!(|| {
        if (cond) return false;
        if (cond) return false;
        return true
    });
}
