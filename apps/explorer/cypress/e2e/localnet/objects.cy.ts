// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

Cypress.config('baseUrl', 'http://localhost:3000');

describe('Objects', () => {
    it('can be reached through URL', () => {
        cy.task('faucet')
            .then((address) => cy.task('mint', address))
            .then(({ effects }) => {
                const { objectId } = effects.created![0].reference;
                cy.visit(`/objects/${objectId}`);
                cy.get('#objectID').contains(objectId);
            });
    });

    it('displays an error when no objects', () => {
        cy.visit(`/objects/fakeAddress`);
        cy.get('#errorResult');
    });

    describe('Owned Objects', () => {
        it('link going from address to object and back', () => {
            cy.task('faucet')
                .then((address) => cy.task('mint', address))
                .then(({ certificate, effects }) => {
                    const address = certificate.data.sender;
                    const [nft] = effects.created!;
                    cy.visit(`/addresses/${address}`);

                    // Find a reference to the NFT:
                    cy.contains(nft.reference.objectId.slice(0, 4)).click();
                    cy.get('#objectID').contains(nft.reference.objectId);

                    // Find a reference to the owning address:
                    cy.contains(address).click();
                    cy.get('#addressID').contains(address);
                });
        });
    });
});
