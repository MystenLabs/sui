module a::m;

public struct SomeStruct has drop {
    some_field: u64,
}

public enum SomeEnum has drop {
    PositionalFields(u64, SomeStruct),
}

 public fun match_variant(s: SomeStruct) {
    use a::m::SomeEnum as SE;
    let e = SE::PositionalFields(42, s);
    match (e) {
        SomeEnum::PositionalFields(num, s) => {
            num + s.some_field;
        },
        SomeEnum::PositionalFields(_, _, _) => (),
    }
}
