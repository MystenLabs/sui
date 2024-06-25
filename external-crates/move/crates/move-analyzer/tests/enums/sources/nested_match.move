module Enums::nested_match {

    public enum OuterEnum<T1, T2> has drop {
        PositionalFields(T1, T2),
        NamedFields { field: T2 },
    }

    public enum InnerEnum<L, R> has drop {
        Left(L),
        Right(R),
    }

    public fun nested_match(e: OuterEnum<u64, InnerEnum<u64, u64>>): u64 {
        match (e) {
            OuterEnum::PositionalFields(num, InnerEnum::Left(inner_num)) => num + inner_num,
            OuterEnum::NamedFields { field: InnerEnum::Right(inner_num) } => inner_num,
            _ => 42,
        }
    }
}
