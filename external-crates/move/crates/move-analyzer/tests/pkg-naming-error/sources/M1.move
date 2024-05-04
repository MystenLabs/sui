module PkgNamingError::M1 {
    use SymbolsRenamed::M9;

    public fun foo() {
        M9::pack();
    }
}
