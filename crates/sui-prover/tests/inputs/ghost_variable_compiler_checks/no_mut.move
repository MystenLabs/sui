module 0x42::foo;

use prover::ghost;

public fun foo<T>() {}

#[spec]
public fun foo_spec<T>() {
    ghost::declare_global_mut<T, bool>();
    ghost::declare_global<u64, bool>();
    foo<T>()
}

public fun bar<T>() {
    foo<T>();
}

#[spec]
public fun bar_spec<T>() {
    ghost::declare_global<T, bool>();
    ghost::declare_global<u64, bool>();
    bar<T>()
}
