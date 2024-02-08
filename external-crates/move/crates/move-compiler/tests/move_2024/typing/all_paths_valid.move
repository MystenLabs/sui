module a::m {
    const ZED: Z = Z { f: 0 };
    const VEC: vector<u64> = vector[0];

    public struct X has copy, drop {
        y: Y
    }
    public struct Y has copy, drop {
        z: Z
    }
    public struct Z has copy, drop {
        f: u64
    }

    fun id(x: X): X { x }
    fun ref_unused(_x: &X) { }
    fun deref(x: &X): X { *x }

    fun all_copy(x: X) {
        copy x;
        copy x.y;
        copy x.y.z;
        copy x.y.z.f;
        copy VEC;
        copy ZED.f;
        (copy x).id();
    }

    fun all_move(x: X, x2: X) {
        move x;
        (move x2).id();
    }

    fun all_borrow(x: X) {
        &x;
        &x.y;
        &x.y.z;
        &x.y.z.f;
        &VEC;
        &ZED.f;
        (&x).deref();
        &x.id();
        &x.deref();
    }

    fun all_borrow_mut(mut x: X) {
        &mut x;
        &mut x.y;
        &mut x.y.z;
        &mut x.y.z.f;
        &mut VEC;
        &mut ZED.f;
        (&mut x).deref();
        &mut x.id();
        &mut x.deref();
    }

    fun all_use(x: X) {
        x;
        x.y;
        x.y.z;
        x.y.z.f;
        VEC;
        ZED.f;
        x.id();
    }

    native public fun vborrow<Element>(v: &vector<Element>, i: u64): &Element;
    native public fun vborrow_mut<Element>(v: &mut vector<Element>, i: u64): &mut Element;

    use fun vborrow as vector.borrow;
    // use fun vborrow_mut as vector.borrow_mut;

    fun id_w(w: W): W { w }
    fun deref_w(w: &W): W { *w }

    public struct T has copy, drop {
        u: U,
    }
    public struct U has copy, drop {
        vs: vector<V>,
    }
    public struct V has copy, drop {
        w: W,
    }
    public struct W has copy, drop {
        xs: vector<X>,
    }
    fun all_index_copy(t: T, n: u64, m: u64) {
        copy t;
        copy t.u;
        copy t.u.vs[2];
        copy t.u.vs[n];
        copy t.u.vs[2].w;
        copy t.u.vs[n].w;
        copy t.u.vs[2].w.xs[0];
        copy t.u.vs[2].w.id_w().xs[0]; // `id_w` at wrong type
        copy t.u.vs[2].w.deref_w().id_w().xs[0]; //
        copy t.u.vs[2].w.xs[m];
        copy t.u.vs[n].w.xs[m+n];
        copy t.u.vs[n].w.xs[m+1];
        copy t.u.vs[n].w.xs[m+1].y;
        copy t.u.vs[n].w.xs[m+1].y.z;
        copy t.u.vs[n].w.xs[m+1].deref();
        copy t.u.vs[n].w.xs[m+1].deref().id();
    }

    // fun all_index_move(t: T, t2: T, n: u64, m: u64) {
    //     move t;
    //     (move t).u.vs[2];
    //     (move t).u.vs[n].w;
    //     (copy (move t).u.vs[n]).w;
    //     (copy (move t).u.vs[n]).w.xs[m+1];
    //     (copy (move t).u.vs[n]).w.xs[m+1].y;
    //     (copy (move t).u.vs[n]).w.xs[m+1].y.z;
    //     (move t2).u;
    //     (move t2).u.vs[2];
    //     (move t2).u.vs[2].w;
    //     (move t2).u.vs[2].w.xs[m+1];
    //     (move t2).u.vs[2].w.xs[m+1].y;
    //     (move t2).u.vs[2].w.xs[m+1].y.z;
    //     move t2.u;
    //     (move t2.u).vs[2];
    //     (move t2.u).vs[2].w;
    //     (move t2.u).vs[2].w.xs[m+1];
    //     (move t2.u).vs[2].w.xs[m+1].y;
    //     (move t2.u).vs[2].w.xs[m+1].y.z;
    // }

    // fun all_index_borrow(t: T, t2: T, n: u64, m: u64) {
    //     &t;
    //     &t.u.vs[2];
    //     &t2.u.vs[n].w;
    //     &t2.u.vs[n].w.xs[m+1];
    //     &t2.u.vs[n].w.xs[m+1].y;
    //     &t2.u.vs[n].w.xs[m+1].y.z;
    //     &t2.u;
    //     &t2.u.vs[2];
    //     &t2.u.vs[2].w;
    //     &t2.u.vs[2].w.xs[m+1];
    //     &t2.u.vs[2].w.xs[m+1].ref_unused(); // invalid -- trying to borrow `()`
    //     &t2.u.vs[2].w.xs[m+1].deref();
    //     &(t2.u.vs[2].w.xs[m+1]).deref();
    //     &(&t2.u.vs[2].w.xs[m+1]).deref();
    // }

    // fun all_index_borrow_mut(mut t: T, mut t2: T, n: u64, m: u64) {
    //     &mut t;
    //     &mut t.u.vs[2];
    //     &mut t2.u.vs[n].w;
    //     &mut t2.u.vs[n].w.xs[m+1];
    //     &mut t2.u.vs[n].w.xs[m+1].y;
    //     &mut t2.u.vs[n].w.xs[m+1].y.z;
    //     &mut t2.u;
    //     &mut t2.u.vs[2];
    //     &mut t2.u.vs[2].w;
    //     &mut t2.u.vs[2].w.xs[m+1];
    //     &mut t2.u.vs[2].w.xs[m+1].ref_unused(); // invalid -- trying to borrow `()`
    //     &mut t2.u.vs[2].w.xs[m+1].deref();
    //     (&mut t2.u.vs[2].w.xs[m+1]).deref();
    //     (&mut t2.u.vs[2].w).xs[m+1].deref();
    // }

    // fun all_index_use(t: T, t2: T, n: u64, m: u64) {
    //     t;
    //     t.u.vs[2];
    //     t2.u.vs[n].w;
    //     t2.u.vs[n].w.xs[m+1];
    //     t2.u.vs[n].w.xs[m+1].y;
    //     t2.u.vs[n].w.xs[m+1].y.z;
    //     t2.u;
    //     t2.u.vs[2];
    //     t2.u.vs[2].w;
    //     t2.u.vs[2].w.xs[m+1];
    //     t2.u.vs[2].w.xs[m+1].id();
    // }
}
