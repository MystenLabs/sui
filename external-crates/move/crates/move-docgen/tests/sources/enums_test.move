/// This is a doc comment above an annotation.
#[allow(unused_const)]
module 0x42::m {
    /// This is a doc comment above an enum
    public enum Enum {
        /// This is a doc comment above a variant
        A,
        B(),
        C(u64),
        /// Another doc comment
        D { 
            /// Doc text on variant field
            x: u64 
        },
        E { x: u64, y: u64 },
    }
}

