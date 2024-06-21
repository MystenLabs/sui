module a::m;

public struct SomeStruct has drop {
    some_field: u64,
}

public enum SomeEnum has drop {
    NamedFields{ num1: u64, num2: u64,s: SomeStruct},
}

 public fun match_variant(s: SomeStruct) {
    use a::m::SomeEnum as SE;
    let e = SE::NamedFields { num1: 7, num2: 42, s };
    match (e) {
        SE::NamedFields { num1, num2, mut s } if (*num1 < s.some_field) => {
            s.some_field = num1 + num2;
        },
        SE::NamedFields { num1, .. } => if (num1 < 42) {
            num1;
        },
        _ => ()
    }
}
