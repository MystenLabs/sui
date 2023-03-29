script {
    use 0x2::MissingResource;
    fun missing_resource() {
        MissingResource::f();
    }
}
