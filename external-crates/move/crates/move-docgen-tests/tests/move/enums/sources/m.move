/// This is a doc comment above an annotation.
#[allow(unused)]
module a::m {
    /// This is a doc comment above an enum
    public enum Enum has drop {
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

    public struct X has drop { x: Enum }
    public struct Y(Enum)

    public struct XG { x: GenericEnum<Enum> }
    public struct YG(GenericEnum<Enum>)

    public struct XGG<T> { x: GenericEnum<T> }
    public struct YGG<T>(GenericEnum<T>)

    public struct VecMap<K: copy, V> has copy, drop, store {
        contents: vector<Entry<K, V>>,
    }

    /// An entry in the map
    public struct Entry<K: copy, V> has copy, drop, store {
        key: K,
        value: V,
    }

    /// Doc comments `type_: VecMap<u64, X>`
    fun f(x: VecMap<u64, X>): u64 {
        0
    }
}
