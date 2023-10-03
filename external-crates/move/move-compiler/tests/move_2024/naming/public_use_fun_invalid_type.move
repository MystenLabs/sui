#[defines_primitive(u64)]
module std::utilities {
    public struct X {}

    public fun x_ex(_: &X) {
        abort 0
    }
    public fun vec_ex<T>(_: &vector<T>) {
        abort 0
    }
    public fun u64_ex(_: u64) {
        abort 0
    }

}

// these modules should not be able to declare public use funs on these types since they did
// not define them

module std::x {
    public use fun std::utilities::x_ex as std::utilities::X.ex;
}

module std::vector {
    public use fun std::utilities::vec_ex as vector.ex;
}

module std::u64 {
    public use fun std::utilities::u64_ex as u64.ex;
}
