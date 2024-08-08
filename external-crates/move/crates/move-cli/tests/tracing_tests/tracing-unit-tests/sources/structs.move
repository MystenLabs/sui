module 0x1::packs {

    public struct X(u64, bool, u8) has drop;

    public struct GenX<A, B, C>(A, B, C) has drop;

    #[test]
    fun test_pack_order() {
        let a = 1;
        let b = true;
        let c = 0u8;
        let x = X(a, b, c);
        assert!(x.2 == 0u8, x.0);
    }

    #[test]
    fun test_unpack_order() {
        let a = 1;
        let b = true;
        let c = 0u8;
        let x = X(a, b, c);
        let X(a, b, c) = x;
        assert!(c == 0u8 && b, a);
    }

    #[test]
    fun test_gen_pack_order() {
        let a = 1u64;
        let b = true;
        let c = 0u8;
        let x = GenX(a, b, c);
        assert!(x.2 == 0u8, x.0);
    }

    #[test]
    fun test_gen_unpack_order() {
        let a = 1u64;
        let b = true;
        let c = 0u8;
        let x = GenX(a, b, c);
        let GenX(a, b, c) = x;
        assert!(c == 0u8 && b, a);
    }
}
