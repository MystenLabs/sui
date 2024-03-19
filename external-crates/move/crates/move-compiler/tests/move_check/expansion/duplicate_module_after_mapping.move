// Duplicate modules need to be checked with respect to name=>value mapping

// Both modules named
module K::M1 {}
module k::M1 {}

// Anon, named
module 0x40::M2 {}
module M::M2 {}

// Named, Anon
module K::M3 {}
module 0x19::M3 {}
