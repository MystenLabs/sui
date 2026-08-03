// A module whose name can be taken by a member alias (member aliases must start
// with 'A'..'Z', so only an upper-case module name can conflict with one).
module ImportConflict::UpperDep {

    public struct UpperDepStruct {
    }

    public fun pub_upper() {
    }
}
