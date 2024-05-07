module test::structs {

    struct Empty {}

    public struct Public {}

    struct Simple {
        f: u64,
    }

    struct WithAbilities has key, drop {
        f: u64,
    }

    struct WithPostfixAbilities {
        f: u64,
    } has key, drop;

    struct TwoField {
        f1: u64,
        f2: u64,
    }

    struct SimpleGeneric<T1: key, T2: store + drop + key> {}

    struct SimpleGenericWithAbilities<T1: key, T2: store + drop> has key {}

    struct OneLongGeneric<
        TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT: key,
    > {}

    struct ThreeLongGenerics<
        phantom TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT1: key,
        TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT2: store,
        TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT3: drop,
    > {}

    struct ThreeLongGenericsWithAbilitiesAndFields<
        TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT1: key,
        TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT2: store,
        TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT3: drop,
    > has key, drop {
        f1: u64,
        f2: u64,
    }

    struct NativeShort<T: key> has key;

    struct NativeGenericWithAbilities<
        T1: key,
        T2: store + drop + key,
        T3,
    > has key, drop;

    struct PositionalEmpty()

    struct PositionalFields(Empty, u64)

    struct PositionalFieldsWithAbilities(Empty, u64) has key, store;

    struct PositionalFieldsLong(
        PositionalFieldsWithAbilities,
        PositionalFieldsWithAbilities,
        PositionalFieldsWithAbilities,
        PositionalFieldsWithAbilities,
        PositionalFieldsWithAbilities,
        PositionalFieldsWithAbilities,
        PositionalFieldsWithAbilities,
    )
}
