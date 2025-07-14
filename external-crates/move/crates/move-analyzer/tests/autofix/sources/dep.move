module Autofix::dep {

    public struct PubStruct has copy, drop {
    }

    public fun create_struct(): PubStruct {
        PubStruct { }
    }
}
