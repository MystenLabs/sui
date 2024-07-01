module 0x42::M {

    fun func1() {
        let a = 5;
        let b = 10;

        // Operations with unequal operands (should not trigger lint)
        let _ = a == b;
        let _ = a != b;
        let _ = a > b;
        let _ = a < b;
        let _ = a & b;
        let _ = a | b;
        let _ = a ^ b;
        let _ = a - b;
        let _ = a / b;
 
    }
}
