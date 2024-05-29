module 0x42::M {
    fun exists(): u64 { 0 }

    fun t(_account: &signer) {
        let _ : u64 = exists();
        let _ : bool = ::exists<Self::R>(0x0);
    }
}
