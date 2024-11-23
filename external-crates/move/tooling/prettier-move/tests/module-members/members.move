// options:
// useModuleLabel: true

// correctly adds empty lines:
// - one after the module label
// - one between different members
// - allows glueing similar members

module prettier::members;
use sui::coin::Coin; // glued with other import
use std::string::String; // empty line follows
const I: u8 = 0; // together with const B
const B: u16 = 100; // empty line after
public struct Point(u8) // together with Point2
public struct Point2(u8) // empty line after
public fun call_something() {} // followed by empty line
public fun call_something_else() {} // empty line after
