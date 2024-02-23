// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { readFileSync } from 'fs';

import { Network } from './sui-utils';

/// We assume our config files are in the format: { "packageId": "0x..." }
const parseConfigurationFile = (fileName: string) => {
	try {
		return JSON.parse(readFileSync(`${fileName}.json`, 'utf8'));
	} catch (e) {
		throw new Error(`Missing config file ${fileName}.json`);
	}
};

/**
 * A default configuration
 * You need to call `publish-contracts.ts` before running any functionality
 * depends on it, or update our imports to not use these json files.
 * */
export const CONFIG = {
	/// Look for events every 1s
	POLLING_INTERVAL_MS: 1000,
	DEFAULT_LIMIT: 50,
	NETWORK: (process.env.NETWORK as Network) || 'testnet',
	SWAP_CONTRACT: parseConfigurationFile('escrow-contract'),
	DEMO_CONTRACT: parseConfigurationFile('demo-contract'),
};
