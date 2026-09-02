// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#config
export type PredictNetwork = 'testnet' | 'mainnet';

export type PredictConfig = {
	network: PredictNetwork;
	fullnodeUrl: string;
	packageId: string;
	predictObjectId: string;
	quoteType: string;
	serverUrl: string;
};

// Testnet IDs pinned to the `predict-testnet-4-16` branch. These change at
// Mainnet launch. Source: Contract Information page.
const TESTNET: PredictConfig = {
	network: 'testnet',
	fullnodeUrl: 'https://fullnode.testnet.sui.io:443',
	packageId: '0xf5ea2b3749c65d6e56507cc35388719aadb28f9cab873696a2f8687f5c785138',
	predictObjectId: '0xc8736204d12f0a7277c86388a68bf8a194b0a14c5538ad13f22cbd8e2a38028a',
	// DeepBook Test USDC (DUSDC), 6 decimals.
	quoteType:
		'0xe95040085976bfd54a1a07225cd46c8a2b4e8e2b6732f140a0fc49850ba73e1a::dusdc::DUSDC',
	serverUrl: 'https://predict-server.testnet.mystenlabs.com',
};

// DeepBook Predict has no Mainnet deployment yet, so there is no Mainnet entry
// to select. Add one at launch: every value differs from Testnet, including the
// quote asset, which is a test coin on Testnet and a real asset on Mainnet.
const CONFIGS: Partial<Record<PredictNetwork, PredictConfig>> = {
	testnet: TESTNET,
};

export function predictConfigFor(network: PredictNetwork): PredictConfig {
	const config = CONFIGS[network];
	if (!config) {
		throw new Error(`DeepBook Predict has no ${network} deployment.`);
	}
	return config;
}

// Resolve once at startup and pass the result down, rather than reading the
// network in each module. The examples target Testnet.
export const PREDICT = predictConfigFor('testnet');

// Oracle ID, expiry, and strike are NOT hardcoded. Read a live oracle from the
// Predict server before minting: GET /predicts/:predict_id/oracles.
export type ActiveOracle = {
	oracleId: string; // object ID of the OracleSVI
	expiry: number; // ms timestamp
	strike: number; // fixed-point strike, per oracle scale
};
// docs::/#config
