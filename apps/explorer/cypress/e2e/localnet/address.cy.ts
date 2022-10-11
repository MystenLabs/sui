// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

Cypress.config('baseUrl', 'http://localhost:3000');

describe('Address', () => {
    it('can be directly visted', () => {
        cy.task('faucet').then((address) => {
            cy.visit(`/addresses/${address}`);
            cy.get('#addressID').contains(address);
        });
    });
});
