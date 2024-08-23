// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { IdentifierString } from '@wallet-standard/core';

/** Sui Devnet */
export const SUI_DEVNET_CHAIN = 'sui:devnet';

/** Sui Testnet */
export const SUI_TESTNET_CHAIN = 'sui:testnet';

/** Sui Localnet */
export const SUI_LOCALNET_CHAIN = 'sui:localnet';

/** Sui Mainnet */
export const SUI_MAINNET_CHAIN = 'sui:mainnet';

export const SUI_CHAINS = [
	SUI_DEVNET_CHAIN,
	SUI_TESTNET_CHAIN,
	SUI_LOCALNET_CHAIN,
	SUI_MAINNET_CHAIN,
] as const;

export type SuiChain =
	| typeof SUI_DEVNET_CHAIN
	| typeof SUI_TESTNET_CHAIN
	| typeof SUI_LOCALNET_CHAIN
	| typeof SUI_MAINNET_CHAIN;

/**
 * Utility that returns whether or not a chain identifier is a valid Sui chain.
 * @param chain a chain identifier in the form of `${string}:{$string}`
 */
export function isSuiChain(chain: IdentifierString): chain is SuiChain {
	return SUI_CHAINS.includes(chain as SuiChain);
}
