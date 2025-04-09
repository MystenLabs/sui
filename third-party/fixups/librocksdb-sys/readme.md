# bindings.rs and overlay

The bindings.rs file (currently just in aarch64/linux) is generated from the cargo build script.  It's way easier to just run cargo build on a dummy project that uses the librocksdb-sys crate on the target arch/os that you need the bindings.rs for.  Copy that file into the overlay directory and you're good to go.