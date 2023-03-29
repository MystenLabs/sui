module 0x42::SignerDisplay {
    use std::signer;

    // Test the signer value display in the error trace
    fun f_incorrect(account: &signer) {
        spec {
            assert signer::address_of(account) == @0x1;
        }
    }
}
