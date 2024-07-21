#[allow(ide_path_autocomplete)]
module 0x42::m1 {
    public struct S {} has drop;

    public fun foo(_s: S) {}

    public fun bar(_s: S) {}


    public fun test1(s: S) {
        use fun bar as S.bak;
        s.bak(); // autocompletion to `bak` and `foo`
    }

    public fun test2(s: S) {
        use fun foo as S.bar;
        s.bar(); // auto-completion to only one (shadowed) `bar`
    }

}
