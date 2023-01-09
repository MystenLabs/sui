// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { registerWallet } from '@mysten/wallet-standard';

import { DAppInterface } from './DAppInterface';
import { SuiWallet } from './WalletStandardInterface';

registerWallet(new SuiWallet());

try {
    let lazyDappInterface: DAppInterface | null = null;
    Object.defineProperty(window, 'suiWallet', {
        enumerable: false,
        configurable: false,
        get() {
            if (!lazyDappInterface) {
                lazyDappInterface = new DAppInterface();
            }
            return lazyDappInterface;
        },
    });
} catch (e) {
    // eslint-disable-next-line no-console
    console.warn(
        '[sui-wallet] Unable to attach to window.suiWallet. There are likely multiple copies of the Sui Wallet installed.'
    );
}
