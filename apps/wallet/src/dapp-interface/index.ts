// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { DAppInterface } from './DAppInterface';
import { SuiWallet } from './WalletStandardInterface';

import type { WalletsWindow } from '@mysten/wallet-standard';

declare const window: WalletsWindow;

window.navigator.wallets = window.navigator.wallets || [];
window.navigator.wallets.push(({ register }) => {
    register(new SuiWallet());
});

Object.defineProperty(window, 'suiWallet', {
    enumerable: false,
    configurable: false,
    value: new DAppInterface(),
});
