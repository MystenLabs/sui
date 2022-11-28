// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

Cypress.config('baseUrl', 'http://localhost:3000');

const nftObject = (num: number) => `div#ownedObjects > div:nth-child(${num}) a`;

describe('PaginationWrapper has buttons', () => {
    const paginationContext = '#NFTSection';

    it('to go to the next page', () => {
        cy.task('faucet')
        .then((address) => cy.task('mint', address))
        .then((tx) => {
            if (!('EffectsCert' in tx)) {
                throw new Error('Missing effects cert');
            }
            cy.get(paginationContext).within(() => {
                cy.get('[data-testid=nextBtn]:visible').click();
                cy.get(nftObject(1)).click();
            });
            cy.get('#objectID').contains('Image2');
        });

     
    });

    it('to go to the last page', () => {
        const address = 'ownsAllAddress';
        cy.visit(`/addresses/${address}`);
        cy.get(paginationContext).within(() => {
            cy.get('[data-testid=lastBtn]:visible').click();
            cy.get(nftObject(1)).click();
        });
        cy.get('#objectID').contains('CollectionObject');
    });

    it('to go back a page', () => {
        const address = 'ownsAllAddress';
        cy.visit(`/addresses/${address}`);
        cy.get(paginationContext).within(() => {
            cy.get('[data-testid=lastBtn]:visible').click();
            cy.get('[data-testid=backBtn]:visible').click();
            cy.get(nftObject(1)).click();
        });
        cy.get('#objectID').contains('player5');
    });

    it('to go to first page', () => {
        const address = 'ownsAllAddress';
        cy.visit(`/addresses/${address}`);
        cy.get(paginationContext).within(() => {
            cy.get('[data-testid=lastBtn]:visible').click();
            cy.get('[data-testid=backBtn]:visible').click();
            cy.get('[data-testid=firstBtn]:visible').click();
        });
        cy.get(nftObject(1)).click();
        cy.get('#objectID').contains('ChildObjectWBrokenImage');
    });
});
