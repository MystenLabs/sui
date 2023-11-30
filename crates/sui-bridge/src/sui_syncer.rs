// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The Suisyncer module is responsible for synchronizing Events emitted on Sui blockchain from
//! concerned bridge packages. Each package is associated with a cursor, and the syncer will
//! only query from that cursor onwards. It's likely that SuiSyncer only tracks one package.




pub struct EthSyncer<P> {
    eth_client: Arc<SuiClient>,
    contract_addresses: EthTargetAddresses,
}
