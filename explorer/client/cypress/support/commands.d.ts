// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// <reference types="cypress" />

declare namespace Cypress {
    interface Chainable {
        task(name: 'faucet', arg?: unknown): Chainable<string>;
    }
}
