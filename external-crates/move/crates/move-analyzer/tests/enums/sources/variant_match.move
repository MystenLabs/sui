module Enums::variant_match {

    public struct SomeStruct has drop {
        some_field: u64,
    }

    public enum SomeEnum has drop {
        PositionalFields(u64, SomeStruct),
        NamedFields{ num1: u64, num2: u64, s: SomeStruct},
        Empty,
    }

    public fun variant_match(s: SomeStruct) {
        use Enums::variant_match::SomeEnum as SE;
        let mut e = SE::PositionalFields(42, s);

        let local = 42;

        match (&mut e) {
            SomeEnum::PositionalFields(num, s) => {
                *num = s.some_field;
            },
            SomeEnum::PositionalFields(_, _) => (),
            SE::NamedFields { num1, num2, mut s } if (*num1 < s.some_field && *num1 < local) => {
                s.some_field = *num1 + *num2;
            },
            SE::NamedFields { num1, .. } if (*num1 < 42) =>  {
                num1;
            },
            SE::NamedFields { num1, num2: _, .. } => {
                num1;
            },
            SE::Empty => ()
        }
    }
}
