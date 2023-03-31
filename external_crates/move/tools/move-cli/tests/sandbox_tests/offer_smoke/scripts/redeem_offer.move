// script 2: sender: Carl
script {
use 0x1::M;
use std::offer;

// Carl should *not* be able to claim Alice's offer for Bob
fun redeem_offer(account: signer) {
    M::publish(&account, offer::redeem(&account, @0xA11CE));
}
}
