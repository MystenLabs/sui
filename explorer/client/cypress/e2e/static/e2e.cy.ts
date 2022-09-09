// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

Cypress.config('baseUrl', 'http://localhost:8080');

// Standardized CSS Selectors
const mainBodyCSS = 'main > section > div';
const nftObject = (num: number) => `div#ownedObjects > div:nth-child(${num}) a`;
const ownerButton = 'td#owner > div > div';

// Standardized expectations:
const expectHome = () => {
    cy.get(mainBodyCSS).invoke('attr', 'data-testid').should('eq', 'home-page');
};

const expectErrorResult = () => {
    cy.get(mainBodyCSS).invoke('attr', 'id').should('eq', 'errorResult');
};

const searchText = (text: string) => {
    // TODO: Ideally this should just call `submit` but the search isn't a form yet:
    cy.get('#searchText').type(text).get('#searchBtn').click();
};

describe('End-to-end Tests', () => {
    describe('The Home Page', () => {
        it('is the landing page', () => {
            cy.visit('/');
            expectHome();
        });

        it('is the redirect page', () => {
            cy.visit('/apples');
            expectHome();
        });

        it('has a go home button', () => {
            cy.visit('/objects/CollectionObject');
            cy.get('#homeBtn').click();
            expectHome();
        });
    });

    describe('Wrong Search', () => {
        it('leads to error page', () => {
            cy.visit('/');
            searchText('apples');
            expectErrorResult();
        });
    });

    describe('Object Results', () => {
        const successObjectID = 'CollectionObject';
        const problemObjectID = 'ProblemObject';

        it('can be searched', () => {
            cy.visit('/');
            searchText(successObjectID);
            cy.get('#objectID').contains(successObjectID);
        });

        it('can be reached through URL', () => {
            cy.visit(`/objects/${successObjectID}`);
            cy.get('#objectID').contains(successObjectID);
        });

        it('can have missing data', () => {
            cy.visit(`/objects/${problemObjectID}`);
            expectErrorResult();
        });
    });

    describe('Address Results', () => {
        const successAddressID = 'receiverAddress';
        const noObjectsAddressID = 'senderAddress';

        it('can be searched', () => {
            cy.visit('/');
            searchText(successAddressID);
            cy.get('#addressID').contains(successAddressID);
        });

        it('can be reached through URL', () => {
            cy.visit(`/addresses/${successAddressID}`);
            cy.get('#addressID').contains(successAddressID);
        });

        it('displays error when no objects', () => {
            cy.visit(`/objects/${noObjectsAddressID}`);
            expectErrorResult();
        });
    });

    describe('Transaction Results', () => {
        const successID = 'Da4vHc9IwbvOYblE8LnrVsqXwryt2Kmms+xnJ7Zx5E4=';
        it('can be searched', () => {
            cy.visit('/');
            searchText(successID);
            cy.get('[data-testid=transaction-id]').contains(successID);
        });

        it('can be reached through URL', () => {
            cy.visit(`/transactions/${successID}`);
            cy.get('[data-testid=transaction-id]').contains(successID);
        });

        it('includes the sender time information', () => {
            cy.visit(`/transactions/${successID}`);
            cy.get('[data-testid=transaction-sender]').contains(
                'Sun, 15 Dec 2024 00:00:00 GMT'
            );
        });
    });

    describe('Owned Objects have links that enable', () => {
        it('going from address to object and back', () => {
            cy.visit('/addresses/receiverAddress');
            cy.get(nftObject(1)).click();
            cy.get('#objectID').contains('player1');
            cy.get(ownerButton).click();
            cy.get('#addressID').contains('receiverAddress');
        });

        it('going from object to child object and back', () => {
            cy.visit('/objects/player2');
            cy.get(nftObject(1)).click();
            cy.get('#objectID').contains('Image1');
            cy.get(ownerButton).click();
            cy.get('#objectID').contains('player2');
        });

        it('going from parent to broken image object and back', () => {
            const parentValue = 'ObjectWBrokenChild';
            cy.visit(`/objects/${parentValue}`);
            cy.get(nftObject(1)).click();
            cy.get('#noImage');
            cy.get(ownerButton).click();
            cy.get('#loadedImage');
        });
    });

    describe('PaginationWrapper has buttons', () => {
        const paginationContext = '#NFTSection';

        it('to go to the next page', () => {
            const address = 'ownsAllAddress';
            cy.visit(`/addresses/${address}`);
            cy.get(paginationContext).within(() => {
                cy.get('[data-testid=nextBtn]:visible').click();
                cy.get(nftObject(1)).click();
            });
            cy.get('#objectID').contains('Image2');
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

        it('where last and next disappear in final page', () => {
            const address = 'ownsAllAddress';
            cy.visit(`/addresses/${address}`);
            cy.get(paginationContext).within(() => {
                cy.get('[data-testid=lastBtn]:visible').click();

                //Back and First buttons are not disabled:
                cy.get('[data-testid=backBtn]:visible').should('be.enabled');
                cy.get('[data-testid=firstBtn]:visible').should('be.enabled');

                //Next and Last buttons are disabled:
                cy.get('[data-testid=nextBtn]:visible').should('be.disabled');
                cy.get('[data-testid=lastBtn]:visible').should('be.disabled');
            });
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

        it('where first and back disappear in first page', () => {
            const address = 'ownsAllAddress';
            cy.visit(`/addresses/${address}`);
            cy.get(paginationContext).within(() => {
                //Back and First buttons are disabled:
                cy.get('[data-testid=backBtn]:visible').should('be.disabled');
                cy.get('[data-testid=firstBtn]:visible').should('be.disabled');

                //Next and Last buttons are not disabled:
                cy.get('[data-testid=nextBtn]:visible').should('be.enabled');
                cy.get('[data-testid=lastBtn]:visible').should('be.enabled');
            });
        });
    });

    describe('Group View', () => {
        it('evaluates balance', () => {
            const address = 'ownsAllAddress';
            cy.visit(`/addresses/${address}`);

            // TODO: Add test IDs to make this selection less structural
            cy.get(
                '#groupCollection > div:nth-child(2) > div:nth-child(1) > div'
            )
                .children()
                .eq(1)
                .contains('0x2::USD::USD')
                .next()
                .contains('2')
                .next()
                .contains('9007199254740993');

            cy.get(
                '#groupCollection > div:nth-child(2) > div:nth-child(2) > div'
            )
                .children()
                .eq(1)
                .contains('SUI')
                .next()
                .contains('2')
                .next()
                .contains('200');
        });
    });

    // TODO: This test isn't great, ideally we'd either do some more manual assertions, validate linking,
    // or use visual regression testing.
    describe('Transactions for ID', () => {
        it('are displayed from and to address', () => {
            const address = 'ownsAllAddress';
            cy.visit(`/addresses/${address}`);

            cy.get('[data-testid=tx] td').should('have.length.greaterThan', 0);
        });

        it('are displayed for input and mutated object', () => {
            const address = 'CollectionObject';
            cy.visit(`/addresses/${address}`);

            cy.get('[data-testid=tx] td').should('have.length.greaterThan', 0);
        });
    });
});
