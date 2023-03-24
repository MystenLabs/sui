// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export class LedgerConnectionFailedError extends Error {
    constructor(message: string) {
        super(message);
        Object.setPrototypeOf(this, LedgerConnectionFailedError.prototype);
    }
}

export class LedgerNoTransportMechanismError extends Error {
    constructor(message: string) {
        super(message);
        Object.setPrototypeOf(this, LedgerNoTransportMechanismError.prototype);
    }
}
