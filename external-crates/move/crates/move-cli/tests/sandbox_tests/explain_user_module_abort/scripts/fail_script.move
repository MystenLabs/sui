script {
    use 0x2::Fail;
    fun fail_script() {
        Fail::f();
    }
}
