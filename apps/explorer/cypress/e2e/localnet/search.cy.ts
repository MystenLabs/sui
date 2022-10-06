// Copyright (c) Mysten Labs, Inc.
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

    it('can search for objects', () => {
        cy.task('faucet')
            .then((address) => cy.task('mint', address))
            .then(({ effects }) => {
                const { objectId } = effects.created![0].reference;
                cy.visit('/');
                cy.get('[data-testid=search]').type(objectId).type('{enter}');
                cy.url().should('include', `/objects/${objectId}`);
            });
    });
});
