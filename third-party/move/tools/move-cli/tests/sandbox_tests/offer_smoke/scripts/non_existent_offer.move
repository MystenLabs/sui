script {
use std::offer;
fun non_existent_offer(account: signer) {
    offer::redeem<u64>(&account, @0xA11CE);
    offer::address_of<u64>(@0xA11CE);
}
}
