// options:
// printWidth: 80
// useModuleLabel: true

module prettier::modifiers_and_members;

public ( package ) fun a() {}

public (friend) fun b() {}

fun c() {}
native fun d(): u64;
public native fun e(): u64;
macro fun m($f: || -> u64): u64 { $f() }
public struct S {}
native struct N;
