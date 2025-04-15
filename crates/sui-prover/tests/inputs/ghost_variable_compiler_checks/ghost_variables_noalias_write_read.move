module 0x42::foo;

use prover::ghost;

public fun foo<U, V>() {}

#[spec]
public fun foo_spec<U, V>() {
    ghost::declare_global_mut<U, bool>();
    ghost::declare_global<V, bool>();
    foo<U, V>()
}

public fun bar<T>() {
    foo<T, T>();
    foo<u64, u64>();
}

#[spec]
public fun bar_spec<T>() {
    ghost::declare_global_mut<T, bool>();
    ghost::declare_global_mut<u64, bool>();
    bar<T>()
}
