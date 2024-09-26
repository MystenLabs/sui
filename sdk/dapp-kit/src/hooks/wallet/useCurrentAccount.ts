// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletAccount } from '@mysten/wallet-standard';

import { getCurrentAccount } from '../../core/wallet/getCurrentAccount.js';
import { useWalletStore } from './useWalletStore.js';

/**
 * Retrieves the wallet account that is currently selected, if one exists.
 */
export function useCurrentAccount(): WalletAccount | null {
	return useWalletStore(getCurrentAccount);
}
