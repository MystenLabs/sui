// flag: --dependency=tests/sources/functional/script_provider.move
script {
    use 0x1::ScriptProvider;
    use std::signer;

    fun main<Token: store>(account: signer) {
        spec {
            assume signer::address_of(account) == @0x1;
        };
        ScriptProvider::register<Token>(&account);
    }
    spec main {
        aborts_if false;
    }
}
