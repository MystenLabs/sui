module 0x42::foo;

use prover::ghost;

public fun foo<U, V>() {
    ghost::declare_global<U, bool>();
    ghost::declare_global_mut<V, bool>();
}

#[spec_only]
public fun bar<T>() {
    ghost::declare_global_mut<T, bool>();
    ghost::declare_global<u64, bool>();
}
