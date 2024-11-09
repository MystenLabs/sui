//# init --edition 2024.alpha

//# publish
module 0x42::m {

    public struct S has drop { v: vector<u64> }
    public struct Q<T> has drop { ts: vector<T> }

    #[syntax(index)]
    public fun borrow_s(s: &S, i: u64):  &u64 {
        s.v.borrow(i)
    }

    #[syntax(index)]
    public fun borrow_s_mut(s: &mut S, i: u64):  &mut u64 {
        s.v.borrow_mut(i)
    }

    #[syntax(index)]
    public fun borrow_q<T>(q: &Q<T>, i: u64):  &T {
        q.ts.borrow(i)
    }

    #[syntax(index)]
    public fun borrow_q_mut<T>(q: &mut Q<T>, i: u64):  &mut T {
        q.ts.borrow_mut(i)
    }


    public fun make_s(v: vector<u64>):  S {
        S { v }
    }

    public fun make_q<T>(ts: vector<T>):  Q<T> {
        Q { ts }
    }

}

//# run
module 0x43::main {
    use 0x42::m;

    fun main() {
        let v = vector<u64>[0, 1, 2, 3, 4];
        let ts = vector<bool>[true, false, true, false, true];

        let mut s = m::make_s(v);
        let mut q = m::make_q(ts);

        let mut i = 0;

        while (i < 5) {
            assert!(s[i] == i, i);
            assert!(q[i] == (i % 2 == 0), i + 10);

            *(&mut s[i]) = 0;
            *(&mut q[i]) = true;

            i = i + 1;
        };

        i = 0;

        while (i < 5) {
            assert!(s[i] == 0, i + 20);
            assert!(q[i], i + 30);
            i = i + 1;
        };

    }
}

