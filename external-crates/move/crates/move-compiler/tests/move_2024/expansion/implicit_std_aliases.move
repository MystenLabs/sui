// if std is defined, the implicit aliases of
// use std::vector;
// use std::option::{Self, Option};
module a::m {
    public struct S { f: Option<u64> }
    fun wow(): vector<Option<u64>> {
        let mut v = vector::empty();
        vector::push_back(&mut v, option::none());
        v
    }
}
