// these cases need parens
module a::m {
    fun t1() {
        break 'a: { 1 };
    }

    fun t2() {
        return 'a: { 1 };
    }

    fun t3() {
        break 'a: loop {};
    }

    fun t4() {
        return 'a: while (cond) { 1 };
    }
}
