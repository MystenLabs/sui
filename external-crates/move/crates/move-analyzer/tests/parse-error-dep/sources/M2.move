module ParseErrorDep::M2 {
    use ParseErrorDep::M1::SomeStruct;
    use ParseErrorDep::{M1 as M};

    public fun fun_call(): u64 {
        ParseErrorDep::M1::foo()
    }

    public fun struct_access(s: SomeStruct): ParseErrorDep::M1::SomeStruct {
        s
    }
}
