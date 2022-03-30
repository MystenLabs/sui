// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import puppeteer from 'puppeteer';

//Global values:
let browser: any;
let page: any;
const BASE_URL = 'http://localhost:8080';

//Global functions:
const expectHome = async (page: any) => {
    const el = (await page.$('[data-testid="home-page"]')) || false;
    expect(el).not.toBe(false);
};

const expectErrorResult = async (page: any) => {
    const el = await page.$('#errorResult');
    expect(el).not.toBeNull();
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
        const failTransactionID = 'txFails';
        const pendingTransactionID = 'txSendPending';
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
            expect(el).not.toBeNull();
            const value = await page.evaluate((el: any) => el.textContent, el);
            expect(value.trim()).toBe(successTransactionID);
        });

        it('can be reached through URL', async () => {
            await page.goto(`${BASE_URL}/transactions/${successTransactionID}`);
            const el = await page.$('#transactionID');
            expect(el).not.toBeNull();
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

        it('can be pending', async () => {
            await page.goto(`${BASE_URL}/transactions/${pendingTransactionID}`);
            await checkStatus(page, 'pending');
        });
        it('can fail', async () => {
            await page.goto(`${BASE_URL}/transactions/${failTransactionID}`);
            await checkStatus(page, 'fail');
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
            expect(el).not.toBeNull();
            const value = await page.evaluate((el: any) => el.textContent, el);
            expect(value.trim()).toBe(successObjectID);
        });

        it('can be reached through URL', async () => {
            await page.goto(`${BASE_URL}/objects/${successObjectID}`);
            const el = await page.$('#objectID');
            expect(el).not.toBeNull();
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
            expect(el).not.toBeNull();
            const value = await page.evaluate((el: any) => el.textContent, el);
            expect(value.trim()).toBe(successAddressID);
        });

        it('can be reached through URL', async () => {
            await page.goto(`${BASE_URL}/addresses/${successAddressID}`);
            const el = await page.$('#addressID');
            expect(el).not.toBeNull();
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
        it('go from address to object and back', async () => {
            await page.goto(`${BASE_URL}/addresses/receiverAddress`);

            //Click on text saying playerOne:
            const objectLink = await page.$(
                'div#ownedObjects > div:nth-child(1)'
            );
            await objectLink.click();

            //Now on Object Page:
            const objectIDEl = await page.$('#objectID');
            expect(objectIDEl).not.toBeNull();

            //This Object is playerOne:
            const objectValue = await page.evaluate(
                (el: any) => el.textContent,
                objectIDEl
            );
            expect(objectValue.trim()).toBe('playerOne');

            //Click on text saying receiverAddress:
            const receiverAddressLink = await page.$(
                'div#descriptionResults > div:nth-child(5) > span'
            );

            await receiverAddressLink.click();

            //Now on Address Page:
            const addressIDEl = await page.$('#addressID');
            expect(addressIDEl).not.toBeNull();

            //This Address is receiverAddress:
            const addressValue = await page.evaluate(
                (el: any) => el.textContent,
                addressIDEl
            );
            expect(addressValue.trim()).toBe('receiverAddress');
        });
        it('go from object to child object and back', async () => {
            const parentObj = 'playerTwo';
            const childObj = 'standaloneObject';
            await page.goto(`${BASE_URL}/objects/${parentObj}`);

            const objectLink = await page.$(
                'div#ownedObjects > div:nth-child(1)'
            );
            await objectLink.click();
            const objectIDEl = await page.$('#objectID');
            expect(objectIDEl).not.toBeNull();

            const objectValue = await page.evaluate(
                (el: any) => el.textContent,
                objectIDEl
            );
            expect(objectValue.trim()).toBe(childObj);

            const ownerLink = await page.$(
                'div#descriptionResults > div:nth-child(5) > span'
            );

            await ownerLink.click();

            const objectIDEl2 = await page.$('#objectID');
            expect(objectIDEl2).not.toBeNull();

            const objectValue2 = await page.evaluate(
                (el: any) => el.textContent,
                objectIDEl
            );
            expect(objectValue2.trim()).toBe(parentObj);
        });
    });
    describe('Owned Objects have buttons', () => {
        it('to go to the next page', async () => {
            const address = 'ownsAllAddress';
            await page.goto(`${BASE_URL}/addresses/${address}`);
            const btn = await page.$('#nextBtn');
            await btn.click();
            const objectLink = await page.$(
                'div#ownedObjects > div:nth-child(4)'
            );
            await objectLink.click();

            const objectIDEl = await page.$('#objectID');

            const objectValue = await page.evaluate(
                (el: any) => el.textContent,
                objectIDEl
            );
            expect(objectValue.trim()).toBe(
                '0ef165hf64032961fg1g2656h23hgi665jii7690'
            );
        });
        it('to go to the last page', async () => {
            const address = 'ownsAllAddress';
            await page.goto(`${BASE_URL}/addresses/${address}`);
            const btn = await page.$('#lastBtn');
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
            expect(objectValue.trim()).toBe('standaloneObject');
        });
        it('where last and next disappear in final page', async () => {
            const address = 'ownsAllAddress';
            await page.goto(`${BASE_URL}/addresses/${address}`);
            const btn = await page.$('#lastBtn');
            await btn.click();

            expect(await page.$('#nextBtn')).toBeNull();
            expect(await page.$('#lastBtn')).toBeNull();
            expect(await page.$('#backBtn')).not.toBeNull();
            expect(await page.$('#firstBtn')).not.toBeNull();
        });

        it('to go back a page', async () => {
            const address = 'ownsAllAddress';
            await page.goto(`${BASE_URL}/addresses/${address}`);
            await page.$('#lastBtn').then((btn: any) => btn.click());

            await page.$('#backBtn').then((btn: any) => btn.click());

            const objectLink = await page.$(
                'div#ownedObjects > div:nth-child(4)'
            );
            await objectLink.click();

            const objectIDEl = await page.$('#objectID');

            const objectValue = await page.evaluate(
                (el: any) => el.textContent,
                objectIDEl
            );
            expect(objectValue.trim()).toBe('CollectionObject');
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
            expect(objectValue.trim()).toBe(
                '8cd943fd42810749de9e0434f01feg443hgg54v1'
            );
        });
        it('where first and back disappear in first page', async () => {
            const address = 'ownsAllAddress';
            await page.goto(`${BASE_URL}/addresses/${address}`);

            expect(await page.$('#nextBtn')).not.toBeNull();
            expect(await page.$('#lastBtn')).not.toBeNull();
            expect(await page.$('#backBtn')).toBeNull();
            expect(await page.$('#firstBtn')).toBeNull();
        });
    });
});
