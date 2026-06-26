module Import::colon_colon {

    use Import::dep;
    use Import::another_dep;

    // test insertion with imports present
    public fun foo() {
        d           // dep module should not be on the auto-imports list, and neither should private members
    }

    fun baz() {
        dep::bar();
    }

    fun bak() {
        another_dep::AnotherDepStruct
    }

    fun bam() {
        another_dep::De
    }
}
