// there is a parsing error in first module but the following module should still parse even if it
// has attributes specified (fail during typing); let's also make sure that attributes are parsed
// correctly in this kind of situation (by checking that test-only module is unbound)

module 0x42::M1 {
    public fun
}

#[ext(some_annotation)]
module 0x42::M2 {
    public fun wrong_return(): u64 {
    }
}

module 0x42::M3 {
    public fun
}

#[test_only]
module 0x42::M4 {
    public fun foo() {
    }
}

module 0x42::M5 {
    public fun bar() {
        0x42::M4::foo()
    }
}
