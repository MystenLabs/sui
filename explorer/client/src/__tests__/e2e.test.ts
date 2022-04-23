// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import puppeteer from 'puppeteer';

//Global values:
let browser: any;
let page: any;
const BASE_URL = 'http://localhost:8080';

//Global functions:

const checkID = async (page: any, element: string, expected: string) => {
    const id = await page.$eval(element, (el: any) => el.getAttribute('id'));
    expect(id).toBe(expected);
};

const checkDataTestID = async (
    page: any,
    element: string,
    expected: string
) => {
    const id = await page.$eval(element, (el: any) =>
        el.getAttribute('data-testid')
    );
    expect(id).toBe(expected);
};

const checkIsDisabled = async (page: any, element: string) => {
    const id = await page.$eval(element, (el: any) =>
        el.getAttribute('disabled')
    );
    expect(id).toBe('');
};

const checkIsNotDisabled = async (page: any, element: string) => {
    const id = await page.$eval(element, (el: any) =>
        el.getAttribute('disabled')
    );
    expect(id).toBeNull();
};

const expectHome = async (page: any) => {
    await checkDataTestID(page, 'main > div', 'home-page');
};

const expectErrorResult = async (page: any) => {
    await checkID(page, 'main > div', 'errorResult');
};

const searchText = async (page: any, text: string) => {
    await page.type('#searchText', text);
    await page.click('#searchBtn');
};

describe('End-to-end Tests', () => {
    beforeAll(async () => {
        browser = await puppeteer.launch();
        page = await browser.newPage();
    });

    afterAll(async () => {
        browser.close();
    });

    describe('The Home Page', () => {
        it('is the landing page', async () => {
            await page.goto(BASE_URL);
            await expectHome(page);
        });

        it('is the redirect page', async () => {
            await page.goto(`${BASE_URL}/apples`);
            await expectHome(page);
        });

        it('has a go home button', async () => {
            await page.goto(`${BASE_URL}/apples`);
            await page.$eval('#homeBtn', (form: any) => form.click());
            await expectHome(page);
        });
    });

    describe('Wrong Search', () => {
        it('leads to error page', async () => {
            await page.goto(BASE_URL);
            await searchText(page, 'apples');
            await expectErrorResult(page);
        });
    });

    describe('Transaction Results', () => {
        //Specific to transaction tests:
        const successTransactionID = 'txCreateSuccess';
        const missingDataTransactionID = 'txMissingData';

        const checkStatus = async (
            page: any,
            expected: 'success' | 'pending' | 'fail'
        ) => {
            const actual = await page.$eval(
                '#transactionStatus',
                (el: any) => el.textContent
            );
            expect(actual).toBe(expected);
        };

        it('can be searched', async () => {
            await page.goto(BASE_URL);
            await searchText(page, successTransactionID);
            const el = await page.$('#transactionID');
            const value = await page.evaluate((el: any) => el.textContent, el);
            expect(value.trim()).toBe(successTransactionID);
        });

        it('can be reached through URL', async () => {
            await page.goto(BASE_URL);
            await page.goto(`${BASE_URL}/transactions/${successTransactionID}`);
            const el = await page.$('#transactionID');
            const value = await page.evaluate((el: any) => el.textContent, el);
            expect(value.trim()).toBe(successTransactionID);
        });
        it('has correct structure', async () => {
            await page.goto(`${BASE_URL}/transactions/${successTransactionID}`);

            const labels = [
                'Transaction ID',
                'Status',
                'From',
                'Event',
                'Object',
                'To',
            ];

            for (let i = 1; i <= labels.length; i++) {
                const value = await page.$eval(
                    `div#textResults > div:nth-child(${i}) > div:nth-child(1)`,
                    (el: any) => el.textContent
                );
                expect(value.trim()).toBe(labels[i - 1]);
            }
        });

        it('can be a success', async () => {
            await page.goto(`${BASE_URL}/transactions/${successTransactionID}`);
            await checkStatus(page, 'success');
        });

        it('can have missing data', async () => {
            await page.goto(
                `${BASE_URL}/transactions/${missingDataTransactionID}`
            );
            await expectErrorResult(page);
        });
    });

    describe('Object Results', () => {
        const successObjectID = 'CollectionObject';
        const problemObjectID = 'ProblemObject';
        const readOnlyObject = 'ComponentObject';
        const notReadOnlyObject = 'CollectionObject';

        const checkStatus = async (page: any, expected: 'True' | 'False') => {
            const actual = await page.$eval(
                '#readOnlyStatus',
                (el: any) => el.textContent
            );
            expect(actual).toBe(expected);
        };

        it('can be searched', async () => {
            await page.goto(BASE_URL);
            await searchText(page, successObjectID);
            const el = await page.$('#objectID');
            const value = await page.evaluate((el: any) => el.textContent, el);
            expect(value.trim()).toBe(successObjectID);
        });

        it('can be reached through URL', async () => {
            await page.goto(BASE_URL);
            await page.goto(`${BASE_URL}/objects/${successObjectID}`);
            const el = await page.$('#objectID');
            const value = await page.evaluate((el: any) => el.textContent, el);
            expect(value.trim()).toBe(successObjectID);
        });
        it('has correct structure', async () => {
            await page.goto(`${BASE_URL}/objects/${successObjectID}`);

            const labels = [
                'Object ID',
                'Version',
                'Read Only?',
                'Type',
                'Owner',
            ];

            for (let i = 1; i <= labels.length; i++) {
                const value = await page.$eval(
                    `div#descriptionResults > div:nth-child(${i}) > div:nth-child(1)`,
                    (el: any) => el.textContent
                );
                expect(value.trim()).toBe(labels[i - 1]);
            }
        });
        it('can be read only', async () => {
            await page.goto(`${BASE_URL}/objects/${readOnlyObject}`);
            await checkStatus(page, 'True');
        });

        it('can be not read only', async () => {
            await page.goto(`${BASE_URL}/objects/${notReadOnlyObject}`);
            await checkStatus(page, 'False');
        });
        it('can have missing data', async () => {
            await page.goto(`${BASE_URL}/objects/${problemObjectID}`);
            await expectErrorResult(page);
        });
    });

    describe('Address Results', () => {
        const successAddressID = 'receiverAddress';
        const noObjectsAddressID = 'senderAddress';
        it('can be searched', async () => {
            await page.goto(BASE_URL);
            await searchText(page, successAddressID);
            const el = await page.$('#addressID');
            const value = await page.evaluate((el: any) => el.textContent, el);
            expect(value.trim()).toBe(successAddressID);
        });

        it('can be reached through URL', async () => {
            await page.goto(`${BASE_URL}/addresses/${successAddressID}`);
            const el = await page.$('#addressID');
            const value = await page.evaluate((el: any) => el.textContent, el);
            expect(value.trim()).toBe(successAddressID);
        });
        it('has correct structure', async () => {
            await page.goto(`${BASE_URL}/addresses/${successAddressID}`);

            const labels = ['Address ID', 'Owned Objects'];

            for (let i = 1; i <= labels.length; i++) {
                const value = await page.$eval(
                    `div#textResults > div:nth-child(${i}) > div:nth-child(1)`,
                    (el: any) => el.textContent
                );
                expect(value.trim()).toBe(labels[i - 1]);
            }
        });
        it('displays error when no objects', async () => {
            await page.goto(`${BASE_URL}/objects/${noObjectsAddressID}`);
            await expectErrorResult(page);
        });
    });
    describe('Enables clicking links to', () => {
        const navigationTemplate = async (
            page: any,
            parentValue: string,
            parentIsA: 'addresses' | 'objects',
            childValue: string,
            parentToChildNo: number
        ) => {
            await page.goto(`${BASE_URL}/${parentIsA}/${parentValue}`);

            //Click on child in Owned Objects List:
            const objectLink = await page.$(
                `div#ownedObjects > div:nth-child(${parentToChildNo})`
            );
            await objectLink.click();

            //Check ID of child object:
            const childIDEl = await page.$('#objectID');
            const childText = await page.evaluate(
                (el: any) => el.textContent,
                childIDEl
            );
            expect(childText.trim()).toBe(childValue);

            //Click on Owner text:
            const ownerLink = await page.$('div#owner > span:first-child');
            await ownerLink.click();

            //Looking for object or address ID?
            const lookingFor =
                parentIsA === 'addresses' ? '#addressID' : '#objectID';

            //Check ID of parent:
            const parentIDEl = await page.$(lookingFor);
            const parentText = await page.evaluate(
                (el: any) => el.textContent,
                parentIDEl
            );
            expect(parentText.trim()).toBe(parentValue);
        };
        it('go from address to object and back', async () => {
            await navigationTemplate(
                page,
                'receiverAddress',
                'addresses',
                'playerOne',
                1
            );
        });
        it('go from object to child object and back', async () => {
            await navigationTemplate(
                page,
                'playerTwo',
                'objects',
                'standaloneObject',
                1
            );
        });
        it('go from parent to broken image object and back', async () => {
            const parentValue = 'ObjectThatOwns';
            await page.goto(`${BASE_URL}/objects/${parentValue}`);

            //Click on child in Owned Objects List:
            const objectLink = await page.$(
                `div#ownedObjects > div:nth-child(2)`
            );
            await objectLink.click();

            // First see Please Wait Message:
            await checkID(
                page,
                'main > div > div:first-child > div > div',
                'pleaseWaitImage'
            );
            await page.waitForFunction(
                () => !document.querySelector('#pleaseWaitImage')
            );

            //Then see No Image Warning:
            await checkID(
                page,
                'main > div > div:first-child > div > div',
                'noImage'
            );

            //Parent Object contains an image:
            const ownerLink = await page.$('div#owner > span:first-child');
            await ownerLink.click();
            await page.waitForFunction(
                () => !document.querySelector('#pleaseWaitImage')
            );
            await checkID(
                page,
                'main > div > div:first-child > div > img',
                'loadedImage'
            );

            //And no No Image / Please Wait message:
            await expect(
                page.$eval('main > div > div:first-child > div > div')
            ).rejects.toThrow(
                'Error: failed to find element matching selector "main > div > div:first-child > div > div"'
            );
        });
    });
    describe('Owned Objects have buttons', () => {
        it('to go to the next page', async () => {
            const address = 'ownsAllAddress';
            await page.goto(`${BASE_URL}/addresses/${address}`);
            const btn = await page.$('#nextBtn');
            await btn.click();
            const objectLink = await page.$(
                'div#ownedObjects > div:nth-child(2)'
            );
            await objectLink.click();

            const objectIDEl = await page.$('#objectID');

            const objectValue = await page.evaluate(
                (el: any) => el.textContent,
                objectIDEl
            );
            expect(objectValue.trim()).toBe('playerTwo');
        });
        it('to go to the last page', async () => {
            const address = 'ownsAllAddress';
            await page.goto(`${BASE_URL}/addresses/${address}`);
            const btn = await page.$('#lastBtn');
            await btn.click();
            const objectLink = await page.$(
                'div#ownedObjects > div:nth-child(1)'
            );
            await objectLink.click();

            const objectIDEl = await page.$('#objectID');

            const objectValue = await page.evaluate(
                (el: any) => el.textContent,
                objectIDEl
            );
            expect(objectValue.trim()).toBe('standaloneObject');
        });
        it('where last and next disappear in final page', async () => {
            const address = 'ownsAllAddress';
            await page.goto(`${BASE_URL}/addresses/${address}`);
            const btn = await page.$('#lastBtn');
            await btn.click();

            //Back and First buttons are not disabled:
            await checkIsNotDisabled(page, '#backBtn');
            await checkIsNotDisabled(page, '#firstBtn');
            //Next and Last buttons are disabled:
            await checkIsDisabled(page, '#nextBtn');
            await checkIsDisabled(page, '#lastBtn');
        });

        it('to go back a page', async () => {
            const address = 'ownsAllAddress';
            await page.goto(`${BASE_URL}/addresses/${address}`);
            await page.$('#lastBtn').then((btn: any) => btn.click());

            await page.$('#backBtn').then((btn: any) => btn.click());

            const objectLink = await page.$(
                'div#ownedObjects > div:nth-child(2)'
            );
            await objectLink.click();

            const objectIDEl = await page.$('#objectID');

            const objectValue = await page.evaluate(
                (el: any) => el.textContent,
                objectIDEl
            );
            expect(objectValue.trim()).toBe('playerTwo');
        });

        it('to go to first page', async () => {
            const address = 'ownsAllAddress';
            await page.goto(`${BASE_URL}/addresses/${address}`);
            await page.$('#lastBtn').then((btn: any) => btn.click());

            await page.$('#backBtn').then((btn: any) => btn.click());

            await page.$('#firstBtn').then((btn: any) => btn.click());

            const objectLink = await page.$(
                'div#ownedObjects > div:nth-child(4)'
            );
            await objectLink.click();

            const objectIDEl = await page.$('#objectID');

            const objectValue = await page.evaluate(
                (el: any) => el.textContent,
                objectIDEl
            );
            expect(objectValue.trim()).toBe('ComponentObject');
        });
        it('where first and back disappear in first page', async () => {
            const address = 'ownsAllAddress';
            await page.goto(`${BASE_URL}/addresses/${address}`);

            //Next and Last buttons are not disabled:
            await checkIsNotDisabled(page, '#nextBtn');
            await checkIsNotDisabled(page, '#lastBtn');
            //First and Back buttons are disabled:
            await checkIsDisabled(page, '#firstBtn');
            await checkIsDisabled(page, '#backBtn');
        });
    });
});
