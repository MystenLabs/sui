// options:
// printWidth: 50
// useModuleLabel: true
// autoGroupImports: module

module prettier::test_extend {
    public struct S {}
}

extend module prettier::test_extend {
    public struct J {}
}
