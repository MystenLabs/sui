module A::l {
    use A::m as am;
    use A::mod_deprecated;

    #[allow(unused_function)]
    fun f() {
        am::make_bar();
        am::deprecated_function();

        mod_deprecated::deprecated_function();
        mod_deprecated::make_f();

        l<am::Bar>();

        l<mod_deprecated::F>();
    }

    #[allow(unused_function, unused_type_parameter)]
    fun l<T>() { }
}
