// Runtime check: 'let ... else' against `&T` and `&mut T` subjects auto-borrows
// the pattern, so binders inside the success arm are references that observe
// (and can mutate) the original subject. Also covers (a) returning the `&mut`
// binder for upstream mutation and (b) auto-borrow threading through nested
// constructors.
//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    public struct Wrap<T> has drop { inner: Inner<T> }
    public struct Inner<T> has drop { v: T }

    public enum E<T> has drop {
        Some(Wrap<T>),
        None,
    }

    public fun c(x: u64): ABC<u64> { ABC::C(x) }
    public fun b(): ABC<u64> { ABC::B }

    public fun e_some(v: u64): E<u64> { E::Some(Wrap { inner: Inner { v } }) }
    public fun e_none(): E<u64> { E::None }

    // success: returns a reference into the subject
    public fun read_c(subject: &ABC<u64>): u64 {
        let ABC::C(x) = subject else { return 0 };
        *x
    }

    // success: mutates through the auto-borrowed binder
    public fun bump_c(subject: &mut ABC<u64>) {
        let ABC::C(x) = subject else { return };
        *x = *x + 1;
    }

    // returns the `&mut` binder itself so the caller can mutate the subject
    // through it. else branch must diverge (can't fabricate a `&mut u64`).
    public fun get_c_mut(subject: &mut ABC<u64>): &mut u64 {
        let ABC::C(x) = subject else { abort 0 };
        x
    }

    // auto-borrow threads `&` through nested constructors
    public fun read_nested(subject: &E<u64>): u64 {
        let E::Some(Wrap { inner: Inner { v } }) = subject else { return 0 };
        *v
    }

    // auto-borrow threads `&mut` through nested constructors
    public fun bump_nested(subject: &mut E<u64>) {
        let E::Some(Wrap { inner: Inner { v } }) = subject else { return };
        *v = *v + 1;
    }

}

//# run
module 0x43::main {

    fun main() {
        use 0x42::m::{
            c, b, e_some, e_none,
            read_c, bump_c, get_c_mut,
            read_nested, bump_nested,
        };

        // success path: by-ref read
        let s_c = c(7);
        assert!(read_c(&s_c) == 7, 1);

        // else path: by-ref read
        let s_b = b();
        assert!(read_c(&s_b) == 0, 2);

        // success path: by-mut-ref write
        let mut s = c(10);
        bump_c(&mut s);
        assert!(read_c(&s) == 11, 3);

        // else path: by-mut-ref is a no-op
        let mut s2 = b();
        bump_c(&mut s2);
        assert!(read_c(&s2) == 0, 4);

        // returning the &mut binder: caller mutates through it
        let mut s3 = c(50);
        let r = get_c_mut(&mut s3);
        *r = 60;
        assert!(read_c(&s3) == 60, 5);

        // nested by-ref read: success + else
        let n_s = e_some(123);
        assert!(read_nested(&n_s) == 123, 6);
        let n_n = e_none();
        assert!(read_nested(&n_n) == 0, 7);

        // nested by-mut-ref bump: success mutates, else is no-op
        let mut n_s2 = e_some(7);
        bump_nested(&mut n_s2);
        assert!(read_nested(&n_s2) == 8, 8);
        let mut n_n2 = e_none();
        bump_nested(&mut n_n2);
        assert!(read_nested(&n_n2) == 0, 9);
    }
}
