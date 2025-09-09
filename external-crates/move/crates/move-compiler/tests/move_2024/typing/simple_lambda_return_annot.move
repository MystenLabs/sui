module a::m;

macro fun call($f: || -> bool): bool {
    $f()
}

#[allow(dead_code)]
fun commands(cond: bool) {
    call!(|| {
        if (cond) return false;
        if (cond) return false;
        (return true: u8)
    });
}
