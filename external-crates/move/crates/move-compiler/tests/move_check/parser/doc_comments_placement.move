/// This is documentation for module M.

/// Documentation comments can have multiple blocks.
/** They may use different limiters. */
module 0x42::M {
    /** There can be doc comments on uses. */
    use std::option::Option;

    /// This is f.
    fun f() { }

    /// This is T
    struct T {
        /// This is a field of T.
        f: Option<u64>,
        /// There can be no doc comment after last field.
    }

    /// There can be no doc comment after last module item.
}

/// There can be no doc comment at end of file.
