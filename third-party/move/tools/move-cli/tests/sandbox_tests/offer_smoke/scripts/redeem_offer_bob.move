// script 3, sender: bob
script {
use 0x1::M;
use std::offer;

// Bob should be able to claim Alice's offer for him
fun redeem_offer_bob(account: signer) {
    // claimed successfully
    let redeemed: M::T = offer::redeem(&account, @0xA11CE);

    // offer should not longer exist
    assert!(!offer::exists_at<M::T>(@0xA11CE), 79);

    // create a new offer for Carl
    offer::create(&account, redeemed, @0xCA21);
}
}
