// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/* eslint-disable @typescript-eslint/consistent-type-imports */

/// <reference types="cypress" />

declare namespace Cypress {
    interface Chainable {
        task(name: 'faucet', arg?: unknown): Chainable<string>;
        task(
            name: 'mint',
            address?: string
        ): Chainable<import('@mysten/sui.js').SuiTransactionResponse>;
    }
}
