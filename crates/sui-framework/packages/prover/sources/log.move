module prover::log;

#[spec_only]
public native fun text(x: vector<u8>);

#[spec_only]
public native fun var<T>(x: &T);

#[spec_only]
public native fun ghost<T>();
