module Import::Import {

    public struct SomeStruct {
    }

    fun module_same_name_as_pkg() {
        // must understand that Import is both package name and module name
        // and auto-complete other modules but also offer to auto-import SomeStruct
        // defined here
        let s: Import::
    }

}
