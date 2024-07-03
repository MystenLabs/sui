module Completion::macro_dot {
    public struct SomeStruct has drop {}

    public macro fun foo($_s: SomeStruct, $_param1: u64, $_param2: |u64, u64| -> u64 , $_param3: u64) {
    }

    public fun test(s: SomeStruct) {
        s.
    }

}
