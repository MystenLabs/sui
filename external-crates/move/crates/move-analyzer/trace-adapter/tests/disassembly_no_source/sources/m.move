// Tests a scenario when there is no source file for a given module
// but there is a disasembled bytecode file for this module. Note
// that module m2 does not have source debug info (or source file in the build
// directory) but it has a disassembled bytecode file which is automatically
// used during debugging. It also tests setting breakpoints in bytecode
// files that do not have a corresponding source file.
module disassembly_no_source::m;

use disassembly_no_source::m2::foo;

#[test]
public fun test() {
    foo(42);
}
