// Tests auto-imports whose inserted name would conflict with a name already in
// scope (another alias or a named address): no `use` is inserted, and completions
// fall back to fully qualified paths.
module ImportConflict::conflict {

    fun member_alias_conflict() {
        use ImportConflict::another_dep::AnotherDepStruct as UpperDep;

        pub
    }

    // Same conflict, but with a module alias taking the name.
    fun module_alias_conflict() {
        use ImportConflict::another_dep as UpperDep;

        pub
    }

    // The name of an auto-imported type is taken by another member alias.
    fun member_name_conflict() {
        use ImportConflict::another_dep::AnotherDepStruct as UpperDepStruct;

        let _s: Upper
    }

    // The name of module `ImportConflict` is taken by the package address.
    fun address_conflict() {
        pub
    }
}
