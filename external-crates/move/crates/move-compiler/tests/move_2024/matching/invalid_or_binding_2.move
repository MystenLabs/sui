module 0x42::m {

    public enum Three<T>{
        Tuple(T, T, T)
    }

    fun test<T: drop>(three: Three<T>) {
        match (three) {
            Three::Tuple(x, _, _) | Three::Tuple(_, y, _) | Three::Tuple(_, _, x) => { let _y = x; },
        }
    }

}
