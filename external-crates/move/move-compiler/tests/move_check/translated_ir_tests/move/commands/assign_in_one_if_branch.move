script {
fun main(cond: bool) {
    let x;
    let y;
    if (cond) x = 5 else ();
    if (cond) y = 5;
    x == y;
}
}

// check: COPYLOC_UNAVAILABLE_ERROR
