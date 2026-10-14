// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 108 --simulator

//# run-graphql
{ # Per-key execution errors must not discard successful siblings or unrelated fields.
  addresses: multiGetAddresses(keys: [
    { address: "0x1" },
    {},
    { address: "0x2" },
  ]) { address }
  objects: multiGetObjects(keys: [
    { address: "0x1" },
    { address: "0x2", version: 1, rootVersion: 1 },
    { address: "0x42" },
    { address: "0x2" },
  ]) { address }
  packages: multiGetPackages(keys: [
    { address: "0x1" },
    { address: "0x2", version: 1, atCheckpoint: 0 },
    { address: "0x42" },
    { address: "0x2" },
  ]) { address }
  types: multiGetTypes(keys: [
    "u64",
    "0x2::coin::Coin<u64, u64>",
    "0x42::missing::Type",
    "bool",
  ]) { repr }
}

//# run-graphql
{ # The same Query resolvers are exposed under checkpoints.
  checkpoint(sequenceNumber: 0) {
    query {
      objects: multiGetObjects(keys: [
        { address: "0x1" },
        { address: "0x2", version: 1, rootVersion: 1 },
        { address: "0x2" },
      ]) { address }
    }
  }
}
