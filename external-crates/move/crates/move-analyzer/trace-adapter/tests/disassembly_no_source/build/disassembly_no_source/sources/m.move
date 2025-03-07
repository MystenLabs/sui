// Tests a scenario when there is no source file for a given module
// but there is a disasembled bytecode file for this module.
module disassembly_no_source::m;

use disassembly_no_source::m2::foo;

#[test]
public fun test() {
    foo(42);
}
