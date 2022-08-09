// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type WalletsWindow } from '@solana/wallet-standard';

import { DAppInterface } from './DAppInterface';
import { SuiWallet, type SuiWalletAccount } from './StandardDAppInterface';

Object.defineProperty(window, 'suiWallet', {
    enumerable: false,
    configurable: false,
    value: new DAppInterface(),
});

((window as WalletsWindow<SuiWalletAccount>).navigator.wallets ||= []).push({
    method: 'register',
    wallets: [new SuiWallet()],
    callback() {
        // TODO: Types require a callback, but I don't think we'll do anything with a callback.
        // We should probably try to make it optional.
    },
});
