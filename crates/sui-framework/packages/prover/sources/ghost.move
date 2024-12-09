module prover::ghost;

#[spec_only]
public native fun global<T, U>(): &U;
#[spec_only]
public native fun declare_global<T, U>();
#[spec_only]
public native fun declare_global_mut<T, U>();

#[spec_only]
#[allow(unused)]
native fun havoc_global<T, U>();
