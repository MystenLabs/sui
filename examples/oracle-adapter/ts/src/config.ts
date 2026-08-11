// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#config
// Testnet oracle configuration. Pin the network's package and object IDs, the
// Hermes endpoint, and the feed IDs here rather than inline in call sites, and
// re-read them from the Pyth docs when the provider rotates them. Price feed IDs
// are the same across chains; the Pyth and Wormhole state objects are per
// network. See https://docs.pyth.network/price-feeds/contract-addresses/sui and
// https://pyth.network/developers/price-feed-ids for the canonical values.
export const TESTNET = {
	// A JSON-RPC endpoint. The Pyth SDK needs JSON-RPC; the Mysten Testnet
	// fullnode serves gRPC, so point this at a JSON-RPC provider or your own node.
	rpcUrl: 'https://rpc-testnet.suiscan.xyz',
	// Pyth and Wormhole shared state objects on Sui Testnet.
	pythStateId: '0x243759059f4c3111179da5878c12f68d612c21a8d54d85edc86164bb18be1c7c',
	wormholeStateId: '0x31358d198147da50db32eda2562951d53973a0c0ad5ed738e9b17d88b213d790',
	// Testnet reads the beta Hermes deployment.
	hermesEndpoint: 'https://hermes-beta.pyth.network',
	// Keeper push interval. Each push is a transaction that costs gas plus the
	// Pyth base fee, so trade freshness against cost.
	keeperIntervalMs: 15_000,
	// Price feed IDs (chain-agnostic). Add the feeds your app consumes.
	feeds: {
		'SUI/USD': '0x50c67b3fd225db8912a424dd4baed60ffdde625ed2feaaf283724f9608fea266',
	},
} as const;
// docs::/#config
