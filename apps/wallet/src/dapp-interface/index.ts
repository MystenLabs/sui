// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { DAppInterface } from './DAppInterface';

Object.defineProperty(window, 'suiWallet', {
    enumerable: false,
    configurable: false,
    value: new DAppInterface(),
});
