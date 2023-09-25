// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createContext } from 'react';

import type { WalletStore } from '../walletStore.js';

export const WalletContext = createContext<WalletStore | null>(null);
