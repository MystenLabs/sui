// script 4, sender: bob
script {
use 0x1::M;
use std::offer;

// Bob should be able to reclaim his own offer for Carl
fun reclaim_offer(account: signer) {
    M::publish(&account, offer::redeem<M::T>(&account, @0xB0B));
}
}
