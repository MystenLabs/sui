// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	resolveFloorPriceRule,
	resolveKioskLockRule,
	resolvePersonalKioskRule,
	resolveRoyaltyRule,
} from './tx/rules//resolve';
import { Network, type ObjectArgument, type RuleResolvingParams } from './types';

/**
 * The Transfer Policy rule.
 */
export type TransferPolicyRule = {
	rule: string;
	network: Network;
	packageId: string;
	resolveRuleFunction: (rule: RuleResolvingParams) => ObjectArgument | void;
	hasLockingRule?: boolean;
};

export const royaltyRuleAddress: Record<Network, string> = {
	[Network.TESTNET]: 'bd8fc1947cf119350184107a3087e2dc27efefa0dd82e25a1f699069fe81a585',
	[Network.MAINNET]: '0x434b5bd8f6a7b05fede0ff46c6e511d71ea326ed38056e3bcd681d2d7c2a7879',
};

export const kioskLockRuleAddress: Record<Network, string> = {
	[Network.TESTNET]: 'bd8fc1947cf119350184107a3087e2dc27efefa0dd82e25a1f699069fe81a585',
	[Network.MAINNET]: '0x434b5bd8f6a7b05fede0ff46c6e511d71ea326ed38056e3bcd681d2d7c2a7879',
};

export const floorPriceRuleAddress: Record<Network, string> = {
	[Network.TESTNET]: '0x06f6bdd3f2e2e759d8a4b9c252f379f7a05e72dfe4c0b9311cdac27b8eb791b1',
	[Network.MAINNET]: '',
};

export const personalKioskAddress: Record<Network, string> = {
	[Network.TESTNET]: '0x06f6bdd3f2e2e759d8a4b9c252f379f7a05e72dfe4c0b9311cdac27b8eb791b1',
	[Network.MAINNET]: '',
};

export const testnetRules: TransferPolicyRule[] = [
	{
		rule: `${royaltyRuleAddress[Network.TESTNET]}::royalty_rule::Rule`,
		network: Network.TESTNET,
		packageId: royaltyRuleAddress[Network.TESTNET],
		resolveRuleFunction: resolveRoyaltyRule,
	},
	{
		rule: `${kioskLockRuleAddress[Network.TESTNET]}::kiosk_lock_rule::Rule`,
		network: Network.TESTNET,
		packageId: kioskLockRuleAddress[Network.TESTNET],
		resolveRuleFunction: resolveKioskLockRule,
		hasLockingRule: true,
	},
	{
		rule: `${personalKioskAddress[Network.TESTNET]}::personal_kiosk_rule::Rule`,
		network: Network.TESTNET,
		packageId: personalKioskAddress[Network.TESTNET],
		resolveRuleFunction: resolvePersonalKioskRule,
	},
	{
		rule: `${floorPriceRuleAddress[Network.TESTNET]}::floor_price_rule::Rule`,
		network: Network.TESTNET,
		packageId: floorPriceRuleAddress[Network.TESTNET],
		resolveRuleFunction: resolveFloorPriceRule,
	},
];

// TODO: Create mainnet rules equivalent array.

export const mainnetRules: TransferPolicyRule[] = [
	{
		rule: `${royaltyRuleAddress[Network.MAINNET]}::royalty_rule::Rule`,
		network: Network.MAINNET,
		packageId: royaltyRuleAddress[Network.MAINNET],
		resolveRuleFunction: resolveRoyaltyRule,
	},
	{
		rule: `${kioskLockRuleAddress[Network.MAINNET]}::kiosk_lock_rule::Rule`,
		network: Network.MAINNET,
		packageId: kioskLockRuleAddress[Network.MAINNET],
		resolveRuleFunction: resolveKioskLockRule,
		hasLockingRule: true,
	},
	// TODO: Add 2 more rules once we do the package upgrade on mainnet.
];

export const rules: TransferPolicyRule[] = [...testnetRules, ...mainnetRules];
