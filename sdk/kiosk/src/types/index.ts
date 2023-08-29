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
	MAINNET,
	TESTNET,
}

/**
 * The Client Options for Both KioskClient & TransferPolicyClient.
 */
export type KioskClientOptions = {
	client: SuiClient;
	network: Network;
};
