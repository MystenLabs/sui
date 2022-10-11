// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { webcrypto } from 'crypto';

if (!globalThis.defined) {
    globalThis.crypto = webcrypto as Crypto;
}
