// Auto-borrow on a 'let ... else' subject must thread the reference mutability
// through nested patterns, so binders below the top-level constructor are also
// references into the subject.
module 0x42::m {

    public struct Wrap<T> has drop { inner: Inner<T> }
    public struct Inner<T> has drop { v: T }

    public enum E<T> has drop {
        Some(Wrap<T>),
        None,
    }

    fun read(subject: &E<u64>): &u64 {
        let E::Some(Wrap { inner: Inner { v } }) = subject else { abort 0 };
        v
    }

    fun read_mut(subject: &mut E<u64>): &mut u64 {
        let E::Some(Wrap { inner: Inner { v } }) = subject else { abort 0 };
        v
    }

}
