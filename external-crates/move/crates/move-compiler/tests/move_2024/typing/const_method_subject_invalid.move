module 0x42::m {

    const ZERO: u64 = 0;

    fun add1_mut(n: &mut u64) {
        *n = *n + 1;
    }

    use fun add1_mut as u64.add1_mut;

    fun mut_ref() {
       ZERO.add1_mut()
    }

    fun deref_and_add(n: &u64): u64 {
        *n + 1
    }

    use fun deref_and_add as u64.deref_and_add;

    fun imm_ref(): u64 {
       ZERO.deref_and_add()
    }

}
