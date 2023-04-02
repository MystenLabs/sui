script {
    use 0x2::ResourceExists;
    fun resource_already_exists(account: signer) {
        ResourceExists::f(&account);
    }
}
