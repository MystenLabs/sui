module Import::function_import_convention {

    // Function auto-imports should import the module and qualify the inserted call.
    fun function_auto_import() {
        pub
    }

    // Type auto-imports should still import the type directly and leave the use site unqualified.
    fun type_auto_import() {
        let _s: Pub
    }
}
