module lint_test_pkg::unused_types {
    use sui::tx_context::TxContext;

    struct UNUSED_TYPES has drop {}

    struct UsedType has drop {}

    struct UnusedType has drop {}

    fun init(_otw: UNUSED_TYPES, _ctx: &mut TxContext) {
        // should not label OTW as unused even though it is never packed
    }

    public fun use_type(): UsedType {
        // we define pack = use
        UsedType {}
    }

    // doesn't count as used
    public fun use_no_pack(x: UnusedType): UnusedType {
        x
    }
}
