module package::my_struct;

public struct SomeStruct has drop, copy {
    f: u64
}

public fun crate_struct(v: u64): SomeStruct  {
    SomeStruct {
        f: v
    }
}
