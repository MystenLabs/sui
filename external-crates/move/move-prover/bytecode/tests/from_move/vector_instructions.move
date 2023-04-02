// dep: ../../move-stdlib/sources/vector.move

module 0x42::M {
    use std::vector;
    const ZERO: u8 = 0;

    fun f() {
        let v = vector[ZERO, ZERO];
        let len = vector::length(&v);
        assert!(len == 1, 0);
    }

    spec f {
        aborts_if true;
    }
}
