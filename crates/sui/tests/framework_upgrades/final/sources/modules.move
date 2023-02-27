/**
 * Base test modules to be added to the sui framework in msim builds, to test
 * framework upgrades.  The module contents below represents the framework
 * *after* it has been upgraded.
 */

module sui::msim_extra_1 {
    public fun foo(): u64 {
        42
    }
}

module sui::msim_extra_2 {
    public fun bar(): u64 {
        43
    }
}
