module 0x1::calls {
    const A: u64 = 1;
    const B: vector<u64> = vector[1,2,3];

    public struct X has drop {
        x: u64, 
        y: bool,
    }

    public struct Y<A, B> has drop {
        x: A,
        y: B,
    }

    #[test]
    fun test_call_order() {
        let a = 1u64;
        let b = true;
        let c = 1u8;
        f_test_call_order(a, b, c);
    }

    fun f_test_call_order(_x: u64, _b: bool, _c: u8) { }

    #[test]
    fun test_return_order() {
        let (a, b, c) = f_test_return_order();
        assert!(c == 0u8, a);
        assert!(b, a);
    }

    fun f_test_return_order(): (u64, bool, u8) {
        (1, true, 0u8)
    }

    #[test]
    fun test_call_return_order() {
        let (a, b, c) = f_test_return_order();
        f_test_call_order(a, b, c);
    }

    #[test]
    fun test_complex_nested_calls() {
        f()
    }
  
    fun f() {
        let x = k() + 1;
        if (x > 0) g(x)
    }

    fun k(): u64 {
        1
    }
  
    fun g(x: u64) {
        let y = x as u8;
        let _ = B;
        let x = X { x: A, y: true };
        let j = Y { x, y: true };
        h(i(j));
        h(y)
    }
  
    fun h(_y: u8) { }
  
    fun i<A: drop, B: drop>(y: Y<A, B>): u8 { 
        let Y { x: _, y: _ } = y;
        let x = &1;
        let _h = *x;
        let j = &mut 1;
        *j = 2;
        *j
    }
}
