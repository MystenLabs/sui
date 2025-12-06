module Dep::m;

#[verify_only]
public struct Bar() has drop;


public fun make_bar(): Bar {
    Bar()
}

#[verify_only]
public fun verify_fun() { }
