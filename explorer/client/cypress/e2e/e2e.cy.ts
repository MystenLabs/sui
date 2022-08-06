// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Standardized CSS Selectors
const mainBodyCSS = 'main > section > div';
const nftObject = (num: number) => `div#ownedObjects > div:nth-child(${num}) a`;
const ownerButton = 'td#owner span:first-child';

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
            cy.visit('/apples');
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

    /*
    describe('Transaction Results', () => {
        // disabled because we are not do not display the Word Transaction ID
        const successID = 'Da4vHc9IwbvOYblE8LnrVsqXwryt2Kmms+xnJ7Zx5E4=';
        it('can be searched', async () => {
            await page.goto(BASE_URL);
            await searchText(page, successID);
            const value = await cssInteract(page)
                .with('#transactionID')
                .get.textContent();
            expect(value.trim()).toBe(successID);
        });

        it('can be reached through URL', async () => {
            await page.goto(`${BASE_URL}/transactions/${successID}`);
            const value = await cssInteract(page)
                .with('#transactionID')
                .get.textContent();
            expect(value.trim()).toBe(successID);
        });
        it('correctly renders days and hours', async () => {
            await page.goto(`${BASE_URL}/transactions/${successID}`);
            const value = await cssInteract(page)
                .with('#timestamp')
                .get.textContent();
            expect(value.trim()).toBe(
                '17 days 1 hour ago (15 Dec 2024 00:00:00 UTC)'
            );
        });
        it('correctly renders a time on the cusp of a year', async () => {
            const otherID = 'GHTP9gcFmF5KTspnz3KxXjvSH8Bx0jv68KFhdqfpdK8=';
            await page.goto(`${BASE_URL}/transactions/${otherID}`);
            const value = await cssInteract(page)
                .with('#timestamp')
                .get.textContent();
            expect(value.trim()).toBe(
                '1 min 3 secs ago (01 Jan 2025 01:12:07 UTC)'
            );
        });
        it('correctly renders a time diff of less than 1 sec', async () => {
            const otherID = 'XHTP9gcFmF5KTspnz3KxXjvSH8Bx0jv68KFhdqfpdK8=';
            await page.goto(`${BASE_URL}/transactions/${otherID}`);
            const value = await cssInteract(page)
                .with('#timestamp')
                .get.textContent();
            expect(value.trim()).toBe('< 1 sec ago (01 Jan 2025 01:13:09 UTC)');
        });
    });*/

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
        it('to go to the next page', () => {
            const address = 'ownsAllAddress';
            cy.visit(`/addresses/${address}`);
            cy.get('[data-testid="nextBtn"]').filter(':visible').click();
            cy.get(nftObject(1)).click();
            cy.get('#objectID').contains('Image2');
        });

        it('to go to the last page', () => {
            const address = 'ownsAllAddress';
            cy.visit(`/addresses/${address}`);
            cy.get('[data-testid="lastBtn"]').filter(':visible').click();
            cy.get(nftObject(1)).click();
            cy.get('#objectID').contains('CollectionObject');
        });

        it('where last and next disappear in final page', () => {
            const address = 'ownsAllAddress';
            cy.visit(`/addresses/${address}`);
            cy.get('[data-testid="lastBtn"]').filter(':visible').click();

            //Back and First buttons are not disabled:
            cy.get('[data-testid="backBtn"]')
                .filter(':visible')
                .should('be.enabled');
            cy.get('[data-testid="firstBtn"]')
                .filter(':visible')
                .should('be.enabled');

            //Next and Last buttons are disabled:
            cy.get('[data-testid="nextBtn"]')
                .filter(':visible')
                .should('be.disabled');
            cy.get('[data-testid="lastBtn"]')
                .filter(':visible')
                .should('be.disabled');
        });

        it('to go back a page', () => {
            const address = 'ownsAllAddress';
            cy.visit(`/addresses/${address}`);

            cy.get('[data-testid="lastBtn"]').filter(':visible').click();
            cy.get('[data-testid="backBtn"]').filter(':visible').click();
            cy.get(nftObject(1)).click();
            cy.get('#objectID').contains('player5');
        });

        it('to go to first page', () => {
            const address = 'ownsAllAddress';
            cy.visit(`/addresses/${address}`);
            cy.get('[data-testid="lastBtn"]').filter(':visible').click();
            cy.get('[data-testid="backBtn"]').filter(':visible').click();
            cy.get('[data-testid="firstBtn"]').filter(':visible').click();
            cy.get(nftObject(1)).click();
            cy.get('#objectID').contains('ChildObjectWBrokenImage');
        });

        it('where first and back disappear in first page', () => {
            const address = 'ownsAllAddress';
            cy.visit(`/addresses/${address}`);

            //Back and First buttons are disabled:
            cy.get('[data-testid="backBtn"]').filter(':visible').should('be.disabled');
            cy.get('[data-testid="firstBtn"]').filter(':visible').should('be.disabled');

            //Next and Last buttons are not disabled:
            cy.get('[data-testid="nextBtn"]').filter(':visible').should('be.enabled');
            cy.get('[data-testid="lastBtn"]').filter(':visible').should('be.enabled');
        });
    });

    describe('Group View', () => {
        it('evaluates balance', () => {
            const address = 'ownsAllAddress';
            cy.visit(`/addresses/${address}`);

            cy.get('#groupCollection').contains('0x2::USD::USD');
            cy.get('#groupCollection').contains('9007199254740993');
            cy.get('#groupCollection').contains('SUI');
            cy.get('#groupCollection').contains('200');
        });
    });

    // TODO: This test isn't great, ideally we'd either do some more manual assertions, validate linking,
    // or use visual regression testing.
    describe('Transactions for ID', () => {
        it('are displayed from and to address', () => {
            const address = 'ownsAllAddress';
            cy.visit(`/addresses/${address}`);

            cy.get('[data-testid="tx"] td').should(
                'have.length.greaterThan',
                0
            );
        });

        it('are displayed for input and mutated object', () => {
            const address = 'CollectionObject';
            cy.visit(`/addresses/${address}`);

            cy.get('[data-testid="tx"] td').should(
                'have.length.greaterThan',
                0
            );
        });
    });
});
