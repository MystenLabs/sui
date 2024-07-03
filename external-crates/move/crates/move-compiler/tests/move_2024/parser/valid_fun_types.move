module a::m {
    public struct Cup<phantom T> has drop {}
    public macro fun foo(
        _: ||,
        _: || -> (),
        _: || -> u64,
        _: || -> (u64),
        _: || -> (u64, bool),
        _: |&u64|,
        _: |&u64| -> (),
        _: |&u64| -> u64,
        _: |&u64| -> (u64),
        _: |&u64| -> (u64, bool),
        _: |bool, address|,
        _: |bool, address| -> (),
        _: |bool, address| -> u64,
        _: |bool, address| -> (u64),
        _: |bool, address| -> (u64, bool),
        _: |bool, address| -> (u64, bool, &u64),
        _: || -> || -> ||,
        _: || -> || -> || -> || -> (),
        _: || -> | | -> || -> | | -> u64,
        _: | | -> || -> | | -> || -> (u64),
        _: Cup<||>,
        _: Cup<|| -> u64>,
    ) {}
}
