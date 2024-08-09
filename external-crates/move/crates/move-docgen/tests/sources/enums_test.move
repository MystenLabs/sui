/// This is a doc comment above an annotation.
#[allow(unused)]
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

    public enum GenericEnum<T> {
        A(T),
        B,
    }

    public struct X { x: Enum }
    public struct Y(Enum)

    public struct XG { x: GenericEnum<Enum> }
    public struct YG(GenericEnum<Enum>)

    public struct XGG<T> { x: GenericEnum<T> }
    public struct YGG<T>(GenericEnum<T>)
}

