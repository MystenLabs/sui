module prover::ghost;

public native fun global<T, U>(): &U;
public native fun declare_global<T, U>();
public native fun declare_global_mut<T, U>();
// public native fun global_update<T, U>(x: &U);
