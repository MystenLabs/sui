module bridge::upgrade_policy {

    use bridge::bridge::Bridge;
    use bridge::committee;
    use bridge::committee::BridgeCommittee;

    fun bridge_upgrade(bridge: &Bridge, message: vector<u8>, signatures: vector<vector<u8>>) {
        let verification = committee::verify_signatures(committee, message, signatures);

    }
}