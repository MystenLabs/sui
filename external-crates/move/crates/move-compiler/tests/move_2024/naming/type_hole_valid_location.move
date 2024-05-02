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
        let P<_>(_) = P<_>(0);
        P<_>(x) = P<_>(0);
        x;
        let S<_> { x: _ } = S<_> { x: 0 };
        S<_> { x } = S<_> { x: 0 };
        x;

        let y: _;
        y = 0;
        y;
        let v: vector<_>;
        v = vector[0];
        v;
        (0: _);
        (vector[0]: vector<_>);
        id<_>(0);
        id<vector<_>>(vector<_>[0]);
        X().xid<_>(0);
        X().xid<vector<_>>(vector<_>[0]);
    }

    public struct X() has copy, drop;
    fun id<T>(t: T): T { t }
    fun xid<T>(_: X, t: T): T { t }
}
