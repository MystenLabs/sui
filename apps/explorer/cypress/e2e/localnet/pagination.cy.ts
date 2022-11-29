// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

Cypress.config('baseUrl', 'http://localhost:3000');

describe('PaginationWrapper has buttons', () => {
    const nftObject = (num: number) =>
        `div#ownedObjects > div:nth-child(${num}) a`;
    it('to go to the last page', () => {
        cy.visit('/addresses/ownsAllAddress');
        cy.get('#NFTSection').within(() => {
            cy.get('[data-testid=lastBtn]:visible').click();
            cy.get(nftObject(1)).click();
        });
        cy.get('#objectID').contains('CollectionObject');
    });

    it('to go to the last page', () => {
        cy.visit('/addresses/ownsAllAddress');
        cy.get('#NFTSection').within(() => {
            cy.get('[data-testid=lastBtn]:visible').click();
            cy.get(nftObject(1)).click();
        });
        cy.get('#objectID').contains('CollectionObject');
    });

    it('to go back a page', () => {
        cy.visit('/addresses/ownsAllAddress');
        cy.get('#NFTSection').within(() => {
            cy.get('[data-testid=lastBtn]:visible').click();
            cy.get('[data-testid=backBtn]:visible').click();
            cy.get(nftObject(1)).click();
        });
        cy.get('#objectID').contains('player5');
    });

    it('to go to first page', () => {
        cy.visit('/addresses/ownsAllAddress');
        cy.get('#NFTSection').within(() => {
            cy.get('[data-testid=lastBtn]:visible').click();
            cy.get('[data-testid=backBtn]:visible').click();
            cy.get('[data-testid=firstBtn]:visible').click();
        });
        cy.get(nftObject(1)).click();
        cy.get('#objectID').contains('ChildObjectWBrokenImage');
    });
});
