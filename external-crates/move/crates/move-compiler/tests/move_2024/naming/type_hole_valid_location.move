// valid locations for _
module a::m {
    public struct P<T>(T)
    public struct S<T> { x: T }
    macro fun foo($x: _, $y: vector<_>): (_, &_) {
        $x;
        $y;
        abort 0
    }

    fun t() {
        let mut x;
        let P<_>(_) = P<_>(0u64);
        P<_>(x) = P<_>(0u64);
        x;
        let S<_> { x: _ } = S<_> { x: 0u64 };
        S<_> { x } = S<_> { x: 0 };
        x;

        let y: _;
        y = 0u64;
        y;
        let v: vector<_>;
        v = vector[0u64];
        v;
        (0u64: _);
        (vector[0u64]: vector<_>);
        id<_>(0u64);
        id<vector<_>>(vector<_>[0u64]);
        X().xid<_>(0u64);
        X().xid<vector<_>>(vector<_>[0u64]);
    }

    public struct X() has copy, drop;
    fun id<T>(t: T): T { t }
    fun xid<T>(_: X, t: T): T { t }
}
