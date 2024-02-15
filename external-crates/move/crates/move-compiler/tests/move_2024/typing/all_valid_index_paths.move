
#[defines_primitive(vector)]
module std::vector {
    #[syntax(index)]
    native public fun vborrow<Element>(v: &vector<Element>, i: u64): &Element;
    #[syntax(index)]
    native public fun vborrow_mut<Element>(v: &mut vector<Element>, i: u64): &mut Element;
}

module a::m {

    fun id_w(w: W): W { w }
    fun id(x: X): X { x }

    fun ref_unused(_x: &X) { }

    fun deref_w(w: &W): W { *w }
    fun deref(x: &X): X { *x }

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

    public struct X has copy, drop {
        y: Y
    }
    public struct Y has copy, drop {
        z: Z
    }
    public struct Z has copy, drop {
        f: u64
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
    }

    fun all_index_borrow(t: T, t2: T, n: u64, m: u64) {
        &t;
        &t.u.vs[2];
        &t2.u.vs[n].w;
        &t2.u.vs[n].w.xs[m+1];
        &t2.u.vs[n].w.xs[m+1].y;
        &t2.u.vs[n].w.xs[m+1].y.z;
        &t2.u;
        &t2.u.vs[2];
        &t2.u.vs[2].w;
        &t2.u.vs[2].w.xs[m+1];
        &t2.u.vs[2].w.xs[m+1].deref();
        &(t2.u.vs[2].w.xs[m+1]).deref();
        &(&t2.u.vs[2].w.xs[m+1]).deref();
    }

    fun all_index_borrow_mut(mut t: T, mut t2: T, n: u64, m: u64) {
        &mut t;
        &mut t.u.vs[2];
        &mut t2.u.vs[n].w;
        &mut t2.u.vs[n].w.xs[m+1];
        &mut t2.u.vs[n].w.xs[m+1].y;
        &mut t2.u.vs[n].w.xs[m+1].y.z;
        &mut t2.u;
        &mut t2.u.vs[2];
        &mut t2.u.vs[2].w;
        &mut t2.u.vs[2].w.xs[m+1];
        &mut t2.u.vs[2].w.xs[m+1].deref();
        (&mut t2.u.vs[2].w.xs[m+1]).deref();
        (&mut t2.u.vs[2].w).xs[m+1].deref();
    }

    fun all_index_use(t: T, t2: T, n: u64, m: u64) {
        t;
        t.u.vs[2];
        t2.u.vs[n].w;
        t2.u.vs[n].w.xs[m+1];
        t2.u.vs[n].w.xs[m+1].y;
        t2.u.vs[n].w.xs[m+1].y.z;
        t2.u;
        t2.u.vs[2];
        t2.u.vs[2].w;
        t2.u.vs[2].w.xs[m+1];
        t2.u.vs[2].w.xs[m+1].id();
    }

    fun all_index_move_00(t: T, t2: T) {
        move t;
        (move t2).u;
    }


    fun all_index_move_01(t: T, t2: T) {
        (move t).u.vs[2];
        (move t2).u.vs[2];
    }


    fun all_index_move_02(t: T, t2: T, n: u64) {
        (move t).u.vs[n].w;
        (move t2).u.vs[2];
    }


    fun all_index_move_03(t: T, t2: T, n: u64) {
        (move t).u.vs[n].w;
        (move t2).u.vs[2].w;
    }


    fun all_index_move_04(t: T, t2: T, n: u64, m: u64) {
        ((move t).u.vs[n]).w.xs[m+1];
        (move t2).u.vs[2].w.xs[m+1];
    }

    fun all_index_move_05(t: T, t2: T, n: u64, m: u64) {
        ((move t).u.vs[n]).w.xs[m+1].id();
        (move t2).u.vs[2].w.xs[m+1].id();
    }


    fun all_index_move_06(t: T, t2: T, n: u64, m: u64) {
        ((move t).u.vs[n]).w.xs[m+1].y;
        (move t2).u.vs[2].w.xs[m+1].y;
    }


    fun all_index_move_07(t: T, t2: T, n: u64, m: u64) {
        ((move t).u.vs[n]).w.xs[m+1].y.z;
        (move t2).u.vs[2].w.xs[m+1].y.z;
    }

}
