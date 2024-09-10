module 0x42::M {
    // Test a missing ">" after the type parameters.
    struct S<phantom T1, phantom T2 { }
}
