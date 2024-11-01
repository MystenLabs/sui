module Completion::dot {
    public struct SomeStruct has drop {
        some_field: u64,
    }

    public fun foo(_s: SomeStruct) {
    }

    public fun bar<T>(s: SomeStruct, _param1: u64, _param2: T): SomeStruct {
        s
    }

    fun simple() {
        let s = SomeStruct { some_field: 42 };
        s.;
    }

    public fun aliased() {
        use fun bar as SomeStruct.bak;
        let s = SomeStruct { some_field: 42 };
        s.;
    }

    public(package) fun shadowed() {
        use fun bar as SomeStruct.foo;
        let s = SomeStruct { some_field: 42 };
        s.;
    }


}
