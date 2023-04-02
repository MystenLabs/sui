// Script 1, sender: alice
script {
use std::offer;
fun multi_offer(account: signer) {
    offer::create(&account, 0, @0xA11CE);
    offer::create(&account, 0, @0x4);
}
}
