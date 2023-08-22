// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Network } from './client/kiosk-client';
import {
	RuleResolvingParams,
	resolveFloorPriceRule,
	resolveKioskLockRule,
	resolvePersonalKioskRule,
	resolveRoyaltyRule,
} from './tx/rules-v2';
import { ObjectArgument } from './types';

/**
 * The Transer Policy Rules package address on testnet
 *  * @deprecated Deprecated due to package upgrades. New `KioskClient` manages rules in a different way.
 *  */
export const TESTNET_RULES_PACKAGE_ADDRESS =
	'bd8fc1947cf119350184107a3087e2dc27efefa0dd82e25a1f699069fe81a585';

/**
 * The transfer policy rules package address on mainnet
 * **
 * @deprecated Deprecated due to package upgrades. New `KioskClient` manages rules in a different way.
 */
export const MAINNET_RULES_PACKAGE_ADDRESS =
	'0x434b5bd8f6a7b05fede0ff46c6e511d71ea326ed38056e3bcd681d2d7c2a7879';

export type TransferPolicyRule = {
	rule: string;
	network: Network;
	packageId: string;
	resolveRuleFunction: (rule: RuleResolvingParams) => ObjectArgument | void;
	hasLockingRule?: boolean;
};

export const personalKioskAddress: Record<Network, string> = {
	[Network.TESTNET]: '0x06f6bdd3f2e2e759d8a4b9c252f379f7a05e72dfe4c0b9311cdac27b8eb791b1',
	[Network.MAINNET]: '',
};

export const testnetRules: TransferPolicyRule[] = [
	{
		rule: 'bd8fc1947cf119350184107a3087e2dc27efefa0dd82e25a1f699069fe81a585::royalty_rule::Rule',
		network: Network.TESTNET,
		packageId: 'bd8fc1947cf119350184107a3087e2dc27efefa0dd82e25a1f699069fe81a585',
		resolveRuleFunction: resolveRoyaltyRule,
	},
	{
		rule: 'bd8fc1947cf119350184107a3087e2dc27efefa0dd82e25a1f699069fe81a585::kiosk_lock_rule::Rule',
		network: Network.TESTNET,
		packageId: 'bd8fc1947cf119350184107a3087e2dc27efefa0dd82e25a1f699069fe81a585',
		resolveRuleFunction: resolveKioskLockRule,
		hasLockingRule: true,
	},
	{
		rule: `0x06f6bdd3f2e2e759d8a4b9c252f379f7a05e72dfe4c0b9311cdac27b8eb791b1::personal_kiosk_rule::Rule`,
		network: Network.TESTNET,
		packageId: '0x06f6bdd3f2e2e759d8a4b9c252f379f7a05e72dfe4c0b9311cdac27b8eb791b1',
		resolveRuleFunction: resolvePersonalKioskRule,
	},
	{
		rule: `0x06f6bdd3f2e2e759d8a4b9c252f379f7a05e72dfe4c0b9311cdac27b8eb791b1::floor_price_rule::Rule`,
		network: Network.TESTNET,
		packageId: '0x06f6bdd3f2e2e759d8a4b9c252f379f7a05e72dfe4c0b9311cdac27b8eb791b1',
		resolveRuleFunction: resolveFloorPriceRule,
	},
];

// TODO: Create mainnet rules equivalent array.

export const rules: TransferPolicyRule[] = [...testnetRules];
