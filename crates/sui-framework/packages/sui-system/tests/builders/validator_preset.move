// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui_system::validator_preset;

const VALID_NET_PUBKEY: vector<u8> = vector[
    171, 2, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20,
    167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38,
];

const VALID_WORKER_PUBKEY: vector<u8> = vector[
    171, 3, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20,
    167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38,
];

// prettier-ignore
const VALID_PUBKEY: vector<u8> = x"99f25ef61f8032b914636460982c5cc6f134ef1ddae76657f2cbfec1ebfc8d097374080df6fcf0dcb8bc4b0d8e0af5d80ebbff2b4c599f54f42d6312dfc314276078c1cc347ebbbec5198be258513f386b930d02c2749a803e2330955ebd1a10";
// prettier-ignore
// A valid proof of possession must be generated using the same account address and protocol public key.
// If either VALID_ADDRESS or VALID_PUBKEY changed, PoP must be regenerated using [fn test_proof_of_possession].
const PROOF_OF_POSSESSION: vector<u8> = x"b01cc86f421beca7ab4cfca87c0799c4d038c199dd399fbec1924d4d4367866dba9e84d514710b91feb65316e4ceef43";
const VALID_NET_ADDR: vector<u8> = b"/ip4/127.0.0.1/tcp/80";
const VALID_P2P_ADDR: vector<u8> = b"/ip4/127.0.0.1/udp/80";
const VALID_CONSENSUS_ADDR: vector<u8> = b"/ip4/127.0.0.1/udp/80";
const VALID_WORKER_ADDR: vector<u8> = b"/ip4/127.0.0.1/udp/80";

const VALIDATOR_PRESET_0: vector<vector<u8>> = vector[
    // name
    "test-validator-0",
    // account-address
    x"af76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a",
    // protocol-key
    VALID_PUBKEY,
    // proof_of_possession
    PROOF_OF_POSSESSION,
    // worker-key
    VALID_WORKER_PUBKEY,
    // network-key
    VALID_NET_PUBKEY,
    // network-address
    VALID_NET_ADDR,
    // p2p-address
    VALID_P2P_ADDR,
    // consensus-address
    VALID_CONSENSUS_ADDR,
    // consensus-worker-address
    VALID_WORKER_ADDR,
    // description
    "description 0",
    // image-url
    "_",
    // project-url
    "_",
];

/// Validator Preset 1.
///
/// Generated using the `sui validator make-validator-info` for this specific
/// address. Use the same command to generate the additional validator info for
/// more validators.
const VALIDATOR_PRESET_1: vector<vector<u8>> = vector[
    // name
    "test-validator-1",
    // account-address
    x"4e4d9cfd64ebb1f78dc960300bf4da1d33470050364b4e5f242255709f683ce1",
    // protocol-key
    x"abd118b3ab1b494e59ca5a00ef0ffb905a405eab6eee074f32062a1ea83e47dd34437f3611c463d563aacbf5375104fc059cafab878c69a5f3fefebe4d80036a7f635c41fcb003db44b3350b711d8b00745323a6f842713ff3d613299761f922",
    // proof_of_possession
    x"ab7a0e380cdce33d8b3060c0daced732ee95095128083d8447d07883537eeddc32038efe43b8488562ca507a72eca52f",
    // worker-key
    x"6df112aba07745145a6b3139fd2e8abd05d711833aab5c4ca5e1d55219516957",
    // network-key
    x"00a709052f75c376e21050c4ba1a0785bab58913cb342311885f612070bbe087",
    // network-address
    "/dns/1.1.1.1/tcp/8080/http",
    // p2p-address
    "/dns/1.1.1.1/udp/8084",
    // consensus-address
    "/dns/1.1.1.1/udp/8081",
    // consensus-worker-address
    "/dns/1.1.1.1/udp/8082",
    // description
    "description 1",
    // image-url
    "_",
    // project-url
    "_",
];

/// Validator Preset 2.
///
/// Generated using the `sui validator make-validator-info` for this specific
/// address. Use the same command to generate the additional validator info for
/// more validators.
const VALIDATOR_PRESET_2: vector<vector<u8>> = vector[
    // name
    "test-validator-2",
    // account-address
    x"9093f2a1c75ffa87c51fb7a27f054adb79ac59fbd123017bb756983a5a295d01",
    // protocol-key
    x"ab13e4b159edb3f1c7a971430294f1a8c172b23005642c9a6cfbb349b445bff480d4b1ea9db91483fedef4a7f3dda22c0b029f876ec380538ee70a4d82202a80edd78e0be4ab29b3ac2b8a1c0bef43e4af6723d7d3d13e40c5a2d57f6ad046a0",
    // proof_of_possession
    x"b1231aace208d7fc359b2cd98d9fcb96cdb763e8e1301a4cb36d31677e4c4888eb8d870703028918ffcca17c3da27617",
    // worker-key
    x"271088841442c0af45fbc25aa29d4624c74db9d0d6e533bc040e29271c25bb5d",
    // network-key
    x"7aacd684de63879fb53032dfcf771e1ddd41ef5cab16372441de41f1e0c24a03",
    // network-address
    "/dns/2.2.2.2/tcp/8080/http",
    // p2p-address
    "/dns/2.2.2.2/udp/8084",
    // consensus-address
    "/dns/2.2.2.2/udp/8081",
    // consensus-worker-address
    "/dns/2.2.2.2/udp/8082",
    // description
    "description 2",
    // image-url
    "_",
    // project-url
    "_",
];

/// Validator Preset 3.
///
/// Generated using the `sui validator make-validator-info` for this specific
/// address. Use the same command to generate the additional validator info for
/// more validators.
const VALIDATOR_PRESET_3: vector<vector<u8>> = vector[
    // name
    "test-validator-3",
    // account-address
    x"15852278a26d2d6f5431d0bdfc86ab34ec3abf6aa0209edc5691f14d551e2614",
    // protocol-key
    x"a48a2c7f4f7f5b4830dc9a0a3887a6601fb61903486ce865dc8c5b4ee28f38cdee906a52e8b5bfe5b17ad6452ca9b1fc0100fbf5fa758b50be8334623599dd629333fe9c717edf9495ddb4cef76d3eac41144f599327f7a736b1da74d117331d",
    // proof_of_possession
    x"8f8630d86518f4bfe269d4899bf3ec5a4b3e2a01f38a7044a8a1efba76420e388eafa80a26b56578efec254380bd5d65",
    // worker-key
    x"82110cfec3cc915e073fd42d1c09728bc9f11abebe02834fe670f384e4467040",
    // network-key
    x"2fdb6965c92428590781f358d96c33c2112040ad3919f39d2c48d797f6d88d60",
    // network-address
    "/dns/3.3.3.3/tcp/8080/http",
    // p2p-address
    "/dns/3.3.3.3/udp/8084",
    // consensus-address
    "/dns/3.3.3.3/udp/8081",
    // consensus-worker-address
    "/dns/3.3.3.3/udp/8082",
    // description
    "description 3",
    // image-url
    "_",
    // project-url
    "_",
];

public struct Preset has copy, drop {
    name: vector<u8>,
    account_address: address,
    protocol_pubkey_bytes: vector<u8>,
    proof_of_possession: vector<u8>,
    worker_pubkey_bytes: vector<u8>,
    network_pubkey_bytes: vector<u8>,
    net_address: vector<u8>,
    p2p_address: vector<u8>,
    primary_address: vector<u8>,
    worker_address: vector<u8>,
    description: vector<u8>,
    image_url: vector<u8>,
    project_url: vector<u8>,
}

public fun preset(index: u64): Preset {
    let preset = match (index) {
        0 => VALIDATOR_PRESET_0,
        1 => VALIDATOR_PRESET_1,
        2 => VALIDATOR_PRESET_2,
        3 => VALIDATOR_PRESET_3,
        _ => abort,
    };

    Preset {
        name: preset[0],
        account_address: sui::address::from_bytes(preset[1]),
        protocol_pubkey_bytes: preset[2],
        proof_of_possession: preset[3],
        worker_pubkey_bytes: preset[4],
        network_pubkey_bytes: preset[5],
        net_address: preset[6],
        p2p_address: preset[7],
        primary_address: preset[8],
        worker_address: preset[9],
        description: preset[10],
        image_url: preset[11],
        project_url: preset[12],
    }
}

public fun name(preset: &Preset): vector<u8> {
    preset.name
}

public fun account_address(preset: &Preset): address {
    preset.account_address
}

public fun protocol_pubkey_bytes(preset: &Preset): vector<u8> {
    preset.protocol_pubkey_bytes
}

public fun proof_of_possession(preset: &Preset): vector<u8> {
    preset.proof_of_possession
}

public fun worker_pubkey_bytes(preset: &Preset): vector<u8> {
    preset.worker_pubkey_bytes
}

public fun network_pubkey_bytes(preset: &Preset): vector<u8> {
    preset.network_pubkey_bytes
}

public fun net_address(preset: &Preset): vector<u8> {
    preset.net_address
}

public fun p2p_address(preset: &Preset): vector<u8> {
    preset.p2p_address
}

public fun primary_address(preset: &Preset): vector<u8> {
    preset.primary_address
}

public fun worker_address(preset: &Preset): vector<u8> {
    preset.worker_address
}

public fun description(preset: &Preset): vector<u8> {
    preset.description
}

public fun image_url(preset: &Preset): vector<u8> {
    preset.image_url
}

public fun project_url(preset: &Preset): vector<u8> {
    preset.project_url
}
