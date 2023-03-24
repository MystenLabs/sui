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

// Connection methods from the Ledger transport libraries don't throw well-structured
// errors, so we can use this utility to form more explicit and structured errors
export function convertErrorToLedgerConnectionFailedError(error: unknown) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    return new LedgerConnectionFailedError(
        `Unable to connect to the user's Ledger device: ${errorMessage}`
    );
}

export function getLedgerConnectionErrorMessage(error: unknown) {
    if (error instanceof LedgerConnectionFailedError) {
        return 'Ledger connection failed. Try again.';
    } else if (error instanceof LedgerNoTransportMechanismError) {
        return "Your machine doesn't support USB or HID.";
    }
    return 'Something went wrong. Try again.';
}
