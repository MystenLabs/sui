// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { registerWallet } from '@mysten/wallet-standard';

import { DAppInterface } from './DAppInterface';
import { SuiWallet } from './WalletStandardInterface';

registerWallet(new SuiWallet());

Object.defineProperty(window, 'suiWallet', {
    enumerable: false,
    configurable: false,
    value: new DAppInterface(),
});
