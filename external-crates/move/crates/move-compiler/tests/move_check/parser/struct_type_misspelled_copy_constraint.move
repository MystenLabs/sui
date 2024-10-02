module 0x42::M {
    // Check misspelling of "copy" constraint.
    struct S<T: copyable> { }
}
