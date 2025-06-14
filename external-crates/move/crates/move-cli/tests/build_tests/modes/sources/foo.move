module 0x42::foo;

#[mode(spec, test)]
public fun foo() {
    abort(invalid_code);
}

#[mode(spec)]
public fun bar() {
    abort(invalid_code);
}

#[mode(test)]
public fun bar() {
    abort(invalid_code);
}
