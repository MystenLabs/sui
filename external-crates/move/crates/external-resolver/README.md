External resolvers
==================

This crate contains defines the protocol for external dependency resolution for
the package management system.

The [module documentation](src/lib.rs) for the library describes the details of the protocol.

This crate also defines a simple resolver that simply returns the data it is
passed for each dependency; this is useful for testing and can be used as an
example for implementors of external resolvers.
