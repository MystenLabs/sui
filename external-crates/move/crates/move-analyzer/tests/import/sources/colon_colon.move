module Import::colon_colon {

    use Import::dep;

    public fun foo() {
        d           // dep module should not be on the auto-imports list, and neither should private members
    }

}
