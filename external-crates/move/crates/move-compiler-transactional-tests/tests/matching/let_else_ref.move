// Runtime check: 'let ... else' against `&T` and `&mut T` subjects auto-borrows
// the pattern, so binders inside the success arm are references that observe
// (and can mutate) the original subject.
//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    public fun c(x: u64): ABC<u64> { ABC::C(x) }
    public fun b(): ABC<u64> { ABC::B }

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

}

//# run
module 0x43::main {

    fun main() {
        use 0x42::m::{c, b, read_c, bump_c};

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
    }
}
