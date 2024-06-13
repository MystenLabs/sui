module 0x42::m {

    public enum Option<T> {
        Some(T)
    }

    fun test<T>(opt: &Option<T>) {
        match (opt) {
            Option::Some(Hello) => (),
        }
    }

}
