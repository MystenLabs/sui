// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

Cypress.config('baseUrl', 'http://localhost:3000');

describe('home page', () => {
    it('is the home page', () => {
        cy.visit('/');
        cy.get('[data-testid=home-page]');
    });

    it('redirects home when visiting an unknown route', () => {
        cy.visit('/apples');
        cy.get('[data-testid=home-page]');
    });

    it('has a go home button', () => {
        cy.visit('/transactions');
        cy.get('[data-testid=home-page]').should('not.exist');
        cy.get('[data-testid=nav-logo-button]').click();
        cy.get('[data-testid=home-page]');
    });

    it('displays the validator table', () => {
        cy.visit('/');
        cy.get('[data-testid=validators-table]');
    });

    it('displays the fullnode map', () => {
        cy.visit('/');
        cy.get('[data-testid=fullnode-map]');
    });
});
