module 0x42::dynamic_fields {
    use sui::dynamic_field as field;

    public struct Parent has key {
        id: UID,
    }

    #[allow(unused_field)]
    public struct DFChild has store {
        count: u64
    }

    fun foo(x: &Parent, y: &DFChild): bool {
        field::borrow(&x.id, b"dfchild") == y
    }

    #[spec_only]
    use prover::prover::requires;

    #[spec(prove)]
    fun foo_spec(x: &Parent, y: &DFChild): bool {
        requires(field::exists_with_type<vector<u8>, DFChild>(&x.id, b"dfchild"));
        foo(x, y)
    }
}
