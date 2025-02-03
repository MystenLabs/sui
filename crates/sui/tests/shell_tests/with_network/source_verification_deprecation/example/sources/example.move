/// Module: example
module example::example;

use dependency::dependency::f;

public fun g(): u64 { f() }
