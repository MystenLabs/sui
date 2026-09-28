// '#[test_only]' package constants work from other modules' tests. The '.unused' mode
// compiles with test code included, so a constant used only from cross-module test code
// counts as used here; a plain 'move build' warns (see the move-cli cross_module_constants
// test)

module 0x42::a {

#[test_only]
public(package) const TEST_MAX: u64 = 100;

public(package) const ONLY_TESTED: u64 = 7;

}

module 0x42::b {

#[test_only]
use 0x42::a;

#[test]
fun uses_both() {
    assert!(a::TEST_MAX == 100);
    assert!(a::ONLY_TESTED == 7);
}

}
