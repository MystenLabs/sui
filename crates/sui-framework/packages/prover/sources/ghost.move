module prover::ghost;

#[verify_only]
public native fun global<T, U>(): &U;
#[verify_only]
public native fun declare_global<T, U>();
#[verify_only]
public native fun declare_global_mut<T, U>();

#[verify_only]
#[allow(unused)]
native fun havoc_global<T, U>();
