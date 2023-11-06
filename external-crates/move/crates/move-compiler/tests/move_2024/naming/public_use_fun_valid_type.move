module std::utilities {
    public struct X {}

    public use fun x_ex as X.ex;
    fun x_ex(_: &X) {
        abort 0
    }
    public fun vec_ex<T>(_: &vector<T>) {
        abort 0
    }
    public fun u64_ex(_: u64) {
        abort 0
    }

    public fun dispatch(x: &X, v: &vector<u64>, u: u64) {
        x.ex();
        v.ex();
        u.ex();
    }
}

#[defines_primitive(vector)]
module std::vector {
    public use fun std::utilities::vec_ex as vector.ex;
}

#[defines_primitive(u64)]
module std::u64 {
    public use fun std::utilities::u64_ex as u64.ex;
}
