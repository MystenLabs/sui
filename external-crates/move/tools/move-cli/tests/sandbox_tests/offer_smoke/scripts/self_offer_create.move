// Script 1, seder: alice
script {
use std::offer;
use std::signer;

// Create a self offer containing a u64
fun self_offer_create(account: signer) {
    let sender = signer::address_of(&account);
    offer::create(&account, 7, @0xA11CE);
    assert!(offer::address_of<u64>(sender) == @0xA11CE , 100);
    assert!(offer::redeem(&account, @0xA11CE) == 7, 101);
}
}
