// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithRequiredFeatures } from '@mysten/wallet-standard';

export const SUI_WALLET_NAME = 'Sui Wallet';

export const DEFAULT_STORAGE_KEY = 'sui-dapp-kit:wallet-connection-info';

export const DEFAULT_REQUIRED_FEATURES: (keyof WalletWithRequiredFeatures['features'])[] = [
	'sui:signTransactionBlock',
];
