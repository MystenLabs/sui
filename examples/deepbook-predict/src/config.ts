// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#config
import { getConfig, getDeployment, getUnits } from '@mysten/deepbook-v3/predict';

// DeepBook Predict runs on Testnet only: `getConfig('mainnet')` throws, because
// there is no Mainnet deployment whose IDs it could return. This constant is the
// single place these examples select a network. Change it here and every other
// file follows.
export const NETWORK = 'testnet' as const;

export const FULLNODE_URL = 'https://fullnode.testnet.sui.io:443';

// One underlying is live on this deployment.
export const UNDERLYING = 'BTC';

// The SDK carries the IDs of whichever deployment its release was cut against.
// Assert the name at startup, so a later SDK release that moves Testnet to a new
// deployment fails loudly here rather than quietly trading against a deployment
// these examples were never checked against.
export const EXPECTED_DEPLOYMENT = 'predict-testnet-8-21';

export const DEPLOYMENT = getDeployment(NETWORK);

if (DEPLOYMENT.deployment !== EXPECTED_DEPLOYMENT) {
	throw new Error(
		`Expected DeepBook Predict deployment ${EXPECTED_DEPLOYMENT}, got ` +
			`${DEPLOYMENT.deployment} (chain ${DEPLOYMENT.chainId}, ` +
			`deepbookv3 commit ${DEPLOYMENT.sourceCommit}).`,
	);
}

// Package IDs, the shared registry, protocol config, and pool vault objects, the
// DUSDC quote coin type, and the per-underlying oracle IDs all come from the
// SDK, so no deployment identifier is hardcoded in these examples.
export const CONFIG = getConfig(NETWORK);

// Scale constants the deployment owns: position quantities are whole
// `positionLotSize` lots, amounts are `quoteCoinDecimals`-decimal DUSDC, and
// probabilities, prices, and rates are fixed point at `fixedPointScale`.
export const UNITS = getUnits(NETWORK);
// docs::/#config
