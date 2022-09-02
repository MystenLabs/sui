// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

Cypress.config('baseUrl', 'http://localhost:3000');

describe('search', () => {
    it('can search for an address', () => {
        cy.task('faucet').then((address) => {
            cy.visit('/');
            cy.get('[data-testid=search]').type(address).type('{enter}');
            cy.url().should('include', `/addresses/${address}`);
        });
    });
});
