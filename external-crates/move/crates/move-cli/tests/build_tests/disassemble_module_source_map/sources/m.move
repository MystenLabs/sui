module 0x42::m;

public enum SomeEnum has drop, copy {
    NamedVariant { field: u64 },
    PositionalVariant(u64),
}

public struct SomeStruct<T1, phantom T2> has drop, copy {
    some_field: T1,
}

const SOME_CONST: u64 = 42;

public fun foo(e: SomeEnum, p1: u64, p2: SomeStruct<u64, u8>): (u64, u64) {
    match (e) {
        SomeEnum::NamedVariant { field } => {
            let mut res = field + p1;
            res = res + p2.some_field;
            (res, SOME_CONST)
        },
        SomeEnum::PositionalVariant(field) => (field, field)
    }
}

#[test]
public fun test() {
    let e = SomeEnum::NamedVariant { field: 42 };
    let s = SomeStruct<u64, u8> { some_field: 7 };
    foo(e, 42, s);
}
