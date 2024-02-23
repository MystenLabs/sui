module 0x42::m {

    public enum Two<T>{
        Tuple(T, T)
    }

    fun test<T: drop>(two: Two<T>) {
        match (two) {
            Two::Tuple(x, x) | Two::Tuple(x, _) => { let _y = x; },
        }
    }

}
