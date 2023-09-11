// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Dispatch } from 'react';
import { createContext } from 'react';
import type { WalletState, WalletAction } from '../reducers/walletReducer.js';

export interface WalletProviderContext extends WalletState {
	dispatch: Dispatch<WalletAction>;
}

export const WalletContext = createContext<WalletProviderContext | null>(null);
