module A::OneDep {
    use A::B;
    public fun do_b() {
        B::foo()
    }
    public entry fun aa() {}
}
