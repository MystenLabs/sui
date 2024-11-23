// Test stepping functionality in presence of function calls:
// - with other instructions on the same line a call, step over line in one go
// - with two calls on the same line, step over both in one go
// - with two calls on the same line, step into the first and
//   after stepping out, step over the second
module stepping_call::m;

fun baz(p: u64): u64 {
    p
}

fun bar(p: u64): u64 {
    p
}

fun foo(p: u64): u64 {
    let v1 = p + p + bar(p) + p + p;
    let v2 = baz(p) + bar(p);
    let v3 = baz(p) + bar(p);
    v1 + v2 + v3
}

#[test]
fun test() {
    foo(42);
}
