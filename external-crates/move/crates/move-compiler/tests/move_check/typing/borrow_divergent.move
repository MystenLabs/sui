module 0x42::m {
    fun main1() {
        loop {
           &break;
        }
    }

    fun main2() {
        &{ return };
    }

    fun main3(cond: bool) {
        &(if (cond) return else return);
    }
}
