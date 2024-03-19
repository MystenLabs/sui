module a::m {
    public struct X has copy, drop {
        y: Y
    }
    public struct Y has copy, drop {
        z: Z
    }
    public struct Z has copy, drop {
        f: u64
    }
    fun test(x: X) {
        copy &x;
        copy *&x;
        copy &x.y;
        copy *&x.y;
        copy x.id();
        copy !0;
        copy 0;
        copy 1 + 1;

        move &x;
        move *&x;
        move &x.y;
        move *&x.y;
        move x.id();
        move !0;
        move 0;
        move 1 + 1;
    }

    fun id(x: X): X {
        x
    }
}
