// Tests a scenario when there is no source file for a given module
// but there is a disasembled bytecode file for this module. Note
// that module m2 does not have source map (or source file in the build
// directory) but it has a disassembled bytecode file which is automatically
// used during debugging.
module disassembly_no_source::m;

use disassembly_no_source::m2::foo;

#[test]
public fun test() {
    foo(42);
}
