// Test stepping through macro defined in the same file.
module macro_same_file::m;

public macro fun bar($param1: u64, $f: |u64| -> u64): u64 {
    let mut ret = $param1 + $param1;
    ret = ret + $f(ret);
    ret
}

public fun foo(): u64 {
    let v = bar!(
        1,
        |x| x + x
    );
    bar!(v, |x| x + x)
}

#[test]
public fun test() {
    foo();
}
