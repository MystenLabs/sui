// A local '#[error]' constant may be defined from cross-module constants and used in a
// local abort

module 0x42::a {

public(package) const PREFIX: vector<u8> = b"err: ";

public(package) const NOT_FOUND: vector<u8> = b"not found";

}

module 0x42::b {

use 0x42::a;

#[error]
const ELocal: vector<u8> = a::PREFIX;

public fun get(): vector<u8> { a::NOT_FOUND }

public fun fail() { abort ELocal }

}
