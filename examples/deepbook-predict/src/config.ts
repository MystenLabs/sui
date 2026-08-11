// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#config
// Testnet-only DeepBook Predict IDs, pinned to the `predict-testnet-4-16` branch.
// These change at Mainnet launch. Source: Contract Information page.
export const PREDICT = {
	network: 'testnet' as const,
	packageId: '0xf5ea2b3749c65d6e56507cc35388719aadb28f9cab873696a2f8687f5c785138',
	predictObjectId: '0xc8736204d12f0a7277c86388a68bf8a194b0a14c5538ad13f22cbd8e2a38028a',
	// DeepBook Test USDC (DUSDC), 6 decimals.
	quoteType:
		'0xe95040085976bfd54a1a07225cd46c8a2b4e8e2b6732f140a0fc49850ba73e1a::dusdc::DUSDC',
	serverUrl: 'https://predict-server.testnet.mystenlabs.com',
};

// Oracle ID, expiry, and strike are NOT hardcoded. Read a live oracle from the
// Predict server before minting: GET /predicts/:predict_id/oracles.
export type ActiveOracle = {
	oracleId: string; // object ID of the OracleSVI
	expiry: number; // ms timestamp
	strike: number; // fixed-point strike, per oracle scale
};
// docs::/#config
