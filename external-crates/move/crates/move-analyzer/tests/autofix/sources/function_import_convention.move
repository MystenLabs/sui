module Autofix::function_import_convention {

    // Function quick fixes should import the module and qualify the function call.
    fun function_quick_fix(): Autofix::dep::PubStruct {
        create_struct()
    }

    // Type quick fixes should keep direct member imports.
    fun type_quick_fix(_s: PubStruct) {
    }

    // If the function's module is already imported under an alias, quick fixes should use it.
    fun function_quick_fix_with_alias(): Autofix::dep::PubStruct {
        use Autofix::dep as d;

        create_struct()
    }

    // If the target module name is already taken, quick fixes should not add a conflicting import.
    fun function_quick_fix_with_conflict(): Autofix::dep::PubStruct {
        use Autofix::another_dep as dep;

        create_struct()
    }

    // If the target module's name is taken by a member alias, quick fixes should not
    // add a conflicting import.
    fun function_quick_fix_with_member_alias_conflict() {
        use Autofix::another_dep::AnotherDepStruct as UpperDep;

        create_upper();
    }
}
