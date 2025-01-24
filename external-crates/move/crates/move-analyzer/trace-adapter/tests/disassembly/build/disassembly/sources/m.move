// Test stepping functionality in disassembly view.
// The things to observe are internal variables related
// to macro compilation that are only visisble in disassembly view,
// as well as per-instruction stepping even though virtual frames
// are present due to how macros are handled by the debugger.
module disassembly::m;

public macro fun bar($param1: u64, $f: |u64| -> u64): u64 {
    let mut ret = $param1 + $param1;
    ret = ret + $f(ret);
    ret
}

public fun foo(p: u64): u64 {
    let v1 = p * p;
    let v2 = bar!(
        v1,
        |x| x + x
    );
    bar!(v2, |x| x + x)
}

#[test]
public fun test() {
    foo(42);
}
