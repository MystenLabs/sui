// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SharedObjectRef } from '@mysten/sui.js/bcs';
import { type SuiClient, type SuiObjectRef } from '@mysten/sui.js/client';
import { type TransactionArgument } from '@mysten/sui.js/transactions';

export * from './kiosk';
export * from './transfer-policy';

/**
 * A valid argument for any of the Kiosk functions.
 */
export type ObjectArgument = string | TransactionArgument | SharedObjectRef | SuiObjectRef;

/**
 * A Network selection
 */
export enum Network {
	MAINNET = 'mainnet',
	TESTNET = 'testnet',
}

/**
 * The Client Options for Both KioskClient & TransferPolicyManager.
 */
export type KioskClientOptions = {
	client: SuiClient;
	network: Network;
};
