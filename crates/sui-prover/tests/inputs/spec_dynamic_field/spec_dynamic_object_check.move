module 0x42::dynamic_fields {
    use sui::dynamic_object_field as ofield;

    public struct Parent has key {
        id: UID,
    }

    #[allow(unused_field)]
    public struct DFOChild has key, store {
        id: UID,
        count: u64
    }

    fun foo(x: &Parent, y: &DFOChild): bool {
        ofield::borrow(&x.id, b"dfochild") == y
    }

    #[spec_only]
    use prover::prover::requires;

    #[spec(prove)]
    fun foo_spec(x: &Parent, y: &DFOChild): bool {
        requires(ofield::exists_with_type<vector<u8>, DFOChild>(&x.id, b"dfochild"));
        foo(x, y)
    }
}
