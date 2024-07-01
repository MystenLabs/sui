module 0x1::a {
    #[test]
    fun test1() {
        let x = 1;
        let y = 2;
        let mut res = 0;
        f(&mut res, x, y);
        assert!(res == 3);
    }

    fun f(x: &mut u64, a: u64, b: u64) {
        *x = a + b;
    }

    #[test]
    fun test2() {
        let x = 1;
        let y = 2;
        let mut res = 0;
        g(&mut res, x, y);
        assert!(res == 3);
    }

    fun g(x: &mut u64, a: u64, b: u64) {
        let y = a + b;
        *x = y;
    }

    public struct X(u64, bool) has drop;

    #[test]
    fun test3() {
        let mut x = X(1, false);
        x.0 = 2;
        x.1 = true;
        let X(a, b) = x;
        if (b) assert!(a == 2)
        else assert!(false);
    }

    #[test]
    fun test4() {
        let x = &mut 1;
        *x = 2;
        *x = 3;
        let mut y = vector[*x];
        *y.borrow_mut(0) = 4;
        assert!(*y.borrow(0) == 4);
    }

    #[test]
    fun test5() {
        let x = &mut 1;
        *x = 2;
        *x = 3;
        let mut y = vector[*x];
        *y.borrow_mut(0) = 4;
        assert!(*y.borrow(0) == 4);
        assert!(y.pop_back() == 4);
    }

    #[test]
    fun test6() {
        let x = 1;
        let y = 2;
        let t = h(x, y);
        assert!(t == 3);
    }

    fun h(a: u64, b: u64): u64 {
        a + b
    }

    public struct Y<T> {
        x: u64,
        y: T,
    } has drop;

    #[test]
    fun test7() {
        let mut y = Y { x: 1, y: Y { x: 2, y: Y { x: 3, y: 4 } } };

        assert!(y.y.y.x == 3, y.y.y.x);

        let mut l0 = &mut y.y;
        l0(l0);
        assert!(y.y.y.x == 4, y.y.y.x);
    }

    fun l0(x: &mut Y<Y<u64>>){
        l1(&mut x.y);
    }

    fun l1(x: &mut Y<u64>){
        incr(&mut x.x);
    }

    fun incr(a: &mut u64) {
        *a = *a + 1;
    }
}
