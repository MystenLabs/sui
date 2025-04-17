module prover::ghost;

#[spec_only]
use prover::prover;

#[spec_only]
public native fun global<T, U>(): &U;

#[spec_only]
public native fun set<T, U>(x: &U);

#[spec]
public fun set_spec<T, U>(x: &U) {
  declare_global_mut<T, U>();
  set<T, U>(x);
  prover::ensures(global<T, U>() == x);
}

#[spec_only]
public native fun borrow_mut<T, U>(): &mut U;

#[spec_only]
public native fun declare_global<T, U>();
#[spec_only]
public native fun declare_global_mut<T, U>();

#[spec_only]
#[allow(unused)]
native fun havoc_global<T, U>();
