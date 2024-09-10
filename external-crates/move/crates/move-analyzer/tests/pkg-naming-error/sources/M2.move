module PkgNamingError::M2 {
    use 0xCAFE::M9;

    public fun foo() {
        M9::pack();
    }
}
