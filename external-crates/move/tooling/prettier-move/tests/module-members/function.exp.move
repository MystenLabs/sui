// options:
// printWidth: 40
// useModuleLabel: true

module prettier::function;

// simple function, no body
fun function() {}

// function with body
fun function() {
    let x = 10;
}

// function modifiers are aplied sorted correctly
entry fun function() {}

// public fun
public fun function() {}

// public(package) fun
public(package) fun function() {}

// public(friend) fun
public(friend) fun function() {}

// keeps no body, preserves native keyword
native fun function();

// sorts correctly, public first
public(package) native fun function();

// sorts correctly, public first
public native fun function();

// sorts correctly, public first
public entry fun function() {}

// sorts correctly, public(package) first
public(package) entry fun function() {}

// works as expected, single line
public fun function(a: u8) {}

// still fits on the same line
public fun function(a: vector<u8>) {}

// breaks on arguments
public fun function(
    param1: vector<u8>,
    param2: vector<u8>,
) {}

// breaks on arguments instead of return type
public fun function(
    a: vector<u8>,
): vector<MyStruct> {}

// breaks on arguments instead of type parameters
public fun function<MyStruct>(
    a: vector<u8>,
) {}

// breaks on both arguments and type parameters
public fun function<
    MyStruct,
    MyOtherStruct,
>(
    a: vector<u8>,
    b: vector<u8>,
): vector<MyStruct> {}

// breaks on arguments, type params and return type
public fun function<
    MyStruct,
    MyOtherStruct,
>(
    a: vector<u8>,
    b: vector<u8>,
): Collection<
    MyStruct,
    MyOtherStruct,
> {}

// keeps comments in place
public fun function(
    // comment
    a: vector<u8>, // comment
    // comment
    b: vector<u8>, // comment
    // comment
): vector<MyStruct> {} // comment

// correctly breaks on types and preserves abilities
public fun function<
    Token: store + copy + drop,
>(
    a: vector<u8>,
): Collection<Token> {}

// correctly breaks on tuple return with comments
public fun function(
    a: vector<u8>,
): (
    // comment
    u64, // comment
    vector<u64>, // comment
) {}

// === Misc ===

// removes space
public(friend) fun function() {}

// removes space
public(package) fun function() {}
