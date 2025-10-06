module 0x42::foo;

#[mode(spec, test)]
public fun foo() {
    abort(0)
}

#[mode(spec)]
public fun bar() {
    abort(0)
}

#[mode(test)]
public fun bar() {
    abort(0)
}
