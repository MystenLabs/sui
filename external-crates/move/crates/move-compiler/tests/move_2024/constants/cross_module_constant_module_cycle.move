// A module "cycle" that exists only through constant references is legal: constant references
// create no module dependency. This holds for function-body uses (c <-> d) and for constant
// definitions referencing each other across modules in both directions without a constant
// cycle (a <-> b)

module 0x42::a {

public(package) const W: u64 = 1;

public(package) const X: u64 = 0x42::b::Y + 1;

}

module 0x42::b {

public(package) const Y: u64 = 2;

public(package) const Z: u64 = 0x42::a::W + 1;

}

module 0x42::c {

public(package) const C: u64 = 1;

public fun uses_d(): u64 { 0x42::d::D }

}

module 0x42::d {

public(package) const D: u64 = 2;

public fun uses_c(): u64 { 0x42::c::C }
}
