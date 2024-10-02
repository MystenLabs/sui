module PreTypeErrorDep::M2 {
    use PreTypeErrorDep::M1::SomeStruct;
    use PreTypeErrorDep::{M1 as M};

    public fun fun_call(): u64 {
        PreTypeErrorDep::M1::foo()
    }

    public fun struct_access(s: SomeStruct): PreTypeErrorDep::M1::SomeStruct {
        s
    }
}
