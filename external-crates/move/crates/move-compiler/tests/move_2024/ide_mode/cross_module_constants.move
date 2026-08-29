// Member completion on a module with a package-visible constant lists the constant; a
// rejected (private) cross-module use under IDE mode continues past the error

module 0x42::a {

public(package) const MAX: u64 = 100;

const SECRET: u64 = 7;

public fun secret(): u64 { SECRET }

}

module 0x42::b {

use 0x42::a;

public fun max(): u64 { a::MAX }

public fun steal(): u64 { a::SECRET }

public fun complete(): u64 {
    a::
}

}
