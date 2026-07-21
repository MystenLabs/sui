module Autofix::UpperDep {

    public struct UpperDepStruct has copy, drop {
    }

    public fun create_upper(): UpperDepStruct {
        UpperDepStruct { }
    }
}
