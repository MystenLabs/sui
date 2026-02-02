// Test: Basic module structure with struct, function, and constants
// EBNF: ModuleDefinition, StructDecl, FunctionDecl, ConstantDecl
module 0x42::basic_module;

const MAX_VALUE: u64 = 1000;
const MIN_VALUE: u64 = 0;

public struct Point has copy, drop { x: u64, y: u64 }

public struct Counter has key, store {
    value: u64,
    owner: address,
}

public fun new_point(x: u64, y: u64): Point {
    Point { x, y }
}

fun add_points(p1: Point, p2: Point): Point {
    Point { x: p1.x + p2.x, y: p1.y + p2.y }
}

public(package) fun scale(p: &Point, factor: u64): Point {
    Point { x: p.x * factor, y: p.y * factor }
}

entry fun reset(counter: &mut Counter) {
    counter.value = MIN_VALUE;
}
