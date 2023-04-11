// 1
script {
use 0x1::M;
use std::offer;
use std::signer;

// Alice creates an offer for Bob that contains an M::T resource
fun create_offer(account: signer) {
    let sender = signer::address_of(&account);
    offer::create(&account, M::create(), @0xB0B);
    assert!(offer::exists_at<M::T>(sender), 77);
    assert!(offer::address_of<M::T>(sender) == @0xB0B , 78);
}
}
