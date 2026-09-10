// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#config
import { getConfig, getDeployment, getUnits } from '@mysten/deepbook-v3/predict';

// The SDK carries a deployment record for Testnet and for Mainnet, so `getConfig`
// resolves either. This constant is the single place these examples select a
// network. Change it here and every other file follows. They default to Testnet,
// where the quote coin is a mintable test coin; on Mainnet it is native USDC.
export const NETWORK = 'testnet' as 'testnet' | 'mainnet';

export const FULLNODE_URL =
	NETWORK === 'mainnet'
		? 'https://fullnode.mainnet.sui.io:443'
		: 'https://fullnode.testnet.sui.io:443';

// One underlying is live on this deployment.
export const UNDERLYING = 'BTC';

// The SDK carries the IDs of whichever deployments its release was cut against.
// Assert the name at startup, so a later SDK release that moves a network to a
// new deployment fails loudly here rather than quietly trading against a
// deployment these examples were never checked against.
export const EXPECTED_DEPLOYMENT = {
	testnet: 'deepbook-predict-testnet',
	mainnet: 'deepbook-predict-mainnet',
}[NETWORK];

export const DEPLOYMENT = getDeployment(NETWORK);

if (DEPLOYMENT.deployment !== EXPECTED_DEPLOYMENT) {
	throw new Error(
		`Expected DeepBook Predict deployment ${EXPECTED_DEPLOYMENT}, got ` +
			`${DEPLOYMENT.deployment} (chain ${DEPLOYMENT.chainId}, ` +
			`deepbookv3 commit ${DEPLOYMENT.sourceCommit}).`,
	);
}

// Package IDs, the shared registry, protocol config, and pool vault objects, the
// quote coin type, and the per-underlying oracle IDs all come from the SDK, so
// no deployment identifier is hardcoded in these examples. Always read the quote
// coin from `CONFIG.quoteCoinType`: on Mainnet it is native USDC, and on Testnet
// it is a test coin with the same `usdc::USDC` module path that displays as DUSDC.
export const CONFIG = getConfig(NETWORK);

// Scale constants the deployment owns: position quantities are whole
// `positionLotSize` lots, amounts are `quoteCoinDecimals`-decimal USDC, and
// probabilities, prices, and rates are fixed point at `fixedPointScale`.
export const UNITS = getUnits(NETWORK);
// docs::/#config
