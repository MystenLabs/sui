// Test: Use fun declarations
// EBNF: UseDecl, Use (fun variant)
module 0x42::use_fun_test;

public struct MyStruct has drop { value: u64 }

public fun get_value(s: &MyStruct): u64 {
    s.value
}

public fun set_value(s: &mut MyStruct, v: u64) {
    s.value = v;
}

public fun create(s: &mut MyStruct, value: u64) {
    s.value = value;
}

use fun get_value as MyStruct.get;
use fun set_value as MyStruct.set;
use fun create as MyStruct.init;

fun test_use_fun() {
    let mut s = MyStruct { value: 10 };
    let v = s.get();
    s.set(v + 1);
    s.init(100);
}
