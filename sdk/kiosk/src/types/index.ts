// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SharedObjectRef } from '@mysten/sui.js/bcs';
import { type SuiClient, type SuiObjectRef } from '@mysten/sui.js/client';
import { TransactionObjectArgument } from '@mysten/sui.js/transactions';

import { BaseRulePackageIds } from '../constants';

export * from './kiosk';
export * from './transfer-policy';

/**
 * A valid argument for any of the Kiosk functions.
 */
export type ObjectArgument = string | TransactionObjectArgument | SharedObjectRef | SuiObjectRef;

/**
 * A Network selector.
 * Kiosk SDK supports mainnet & testnet.
 * Pass `custom` for any other network (devnet, localnet).
 */
export enum Network {
	MAINNET = 'mainnet',
	TESTNET = 'testnet',
	CUSTOM = 'custom',
}

/**
 * The Client Options for Both KioskClient & TransferPolicyManager.
 */
export type KioskClientOptions = {
	client: SuiClient;
	network: Network;
	packageIds?: BaseRulePackageIds;
};
