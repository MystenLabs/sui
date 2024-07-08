#[defines_primitive(vector)]
module std::vector {
    #[syntax(index)]
    native public fun vborrow<Element>(v: &vector<Element>, i: u64): &Element;
    #[syntax(index)]
    native public fun vborrow_mut<Element>(v: &mut vector<Element>, i: u64): &mut Element;
}

#[allow(ide_path_autocomplete,ide_dot_autocomplete)]
module a::m {

    public struct A<T>(vector<T>) has drop;
    public struct B<T>(A<T>) has drop;
    public struct C { c: u64, d: u64 } has drop;

    public fun test0(in: B<A<u64>>) {
        let _ = &in.0.0[1]. ;
    }

    public fun test1(in: B<A<u64>>) {
        let _ = &in.0.0[1].0[0]. ;
    }

    public fun test2(in: B<A<C>>) {
        let _ = &in.0.0[1].0[0]. ;
    }
}
