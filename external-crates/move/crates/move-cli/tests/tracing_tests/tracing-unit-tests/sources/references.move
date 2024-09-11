module 0x1::references {

    #[test]
    fun pass_mut_assign_in_other_fn() {
        let x = 1;
        let y = 2;
        let mut res = 0;
        assign_add(&mut res, x, y);
        assert!(res == 3);
    }

    fun assign_add(x: &mut u64, a: u64, b: u64) {
        *x = a + b;
    }

    public struct X(u64, bool) has drop;

    #[test]
    fun test_struct_borrow() {
        let x = &X(1, false);
        assert!(x.0 == 1);
    }

    #[test]
    fun test_vector_mut_borrow() {
        let x = &mut 1;
        *x = 2;
        *x = 3;
        let mut y = vector[*x];
        *y.borrow_mut(0) = 4;
        assert!(*y.borrow(0) == 4, 42);
        assert!(*x == 3, 42)
    }

    #[test]
    fun test_vector_mut_borrow_pop() {
        let x = &mut 1;
        *x = 2;
        *x = 3;
        let mut y = vector[*x];
        *y.borrow_mut(0) = 4;
        assert!(*y.borrow(0) == 4);
        assert!(y.pop_back() == 4);
    }

    public struct Y<T> {
        x: u64,
        y: T,
    } has drop;

    #[test]
    fun nested_struct_reference_mutation() {
        let mut y = Y { x: 1, y: Y { x: 2, y: Y { x: 3, y: 4 } } };

        assert!(y.y.y.x == 3, y.y.y.x);

        let l0 = &mut y.y;
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

