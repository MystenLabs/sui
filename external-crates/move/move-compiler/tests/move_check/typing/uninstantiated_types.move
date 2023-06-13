module 0x42::unused_types {
    struct UnusedType has drop {}

    public fun use_type(): UsedType {
        // we define pack = use
        UsedType {}
    }

    // make sure that defining type after use does not matter
    struct UsedType has drop {}

    // doesn't count as used
    public fun use_no_pack(x: UnusedType): UnusedType {
        x
    }
}
