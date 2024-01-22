// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import SWAP_CONTRACT from './contracts.json';
import { Network } from './sui-utils';

/// A default configuration
export const CONFIG = {
	DEFAULT_LIMIT: 50,
	NETWORK: (process.env.NETWORK as Network) || 'testnet',
	SWAP_CONTRACT,
};
