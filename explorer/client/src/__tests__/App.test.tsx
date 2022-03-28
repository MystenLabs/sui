// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { render, screen, fireEvent } from '@testing-library/react';
import { createMemoryHistory } from 'history';
import {
    MemoryRouter,
    unstable_HistoryRouter as HistoryRouter,
} from 'react-router-dom';

import App from '../app/App';

const expectHome = async () => {
    const el = await screen.findByTestId('home-page');
    expect(el).toBeInTheDocument();
};

const searchText = (text: string) => {
    fireEvent.change(screen.getByPlaceholderText(/Search by ID/i), {
        target: { value: text },
    });
    fireEvent.submit(screen.getByRole('form', { name: /search form/i }));
};

const expectTransactionStatus = async (
    result: 'fail' | 'success' | 'pending'
) => {
    const el = await screen.findByTestId('transaction-status');
    expect(el).toHaveTextContent(result);
};

const expectReadOnlyStatus = async (result: 'True' | 'False') => {
    const el = await screen.findByTestId('read-only-text');
    expect(el).toHaveTextContent(result);
};

const checkObjectId = async (result: string) => {
    const el1 = await screen.findByTestId('object-id');
    expect(el1).toHaveTextContent(result);
};

const checkAddressId = (result: string) => {
    expect(screen.getByTestId('address-id')).toHaveTextContent(result);
};

const successTransactionID = 'txCreateSuccess';
const failTransactionID = 'txFails';
const pendingTransactionID = 'txSendPending';

const problemTransactionID = 'txProblem';

const successObjectID = 'CollectionObject';
const problemObjectID = 'ProblemObject';

const noDataID = 'nonsenseQuery';

const readOnlyObject = 'ComponentObject';
const notReadOnlyObject = 'CollectionObject';

const addressID = 'receiverAddress';
const addressNoObjectsID = 'senderAddress';

const problemAddressID = 'problemAddress';

describe('End-to-end Tests', () => {
    it('renders the home page', async () => {
        render(<App />, { wrapper: MemoryRouter });
        await expectHome();
    });

    describe('Redirects to Home Page', () => {
        it('redirects to home for every unknown path', async () => {
            render(
                <MemoryRouter initialEntries={['/anything']}>
                    <App />
                </MemoryRouter>
            );
            await expectHome();
        });
        it('redirects to home for unknown path by replacing the history', async () => {
            const history = createMemoryHistory({
                initialEntries: ['/anything'],
            });
            render(
                <HistoryRouter history={history}>
                    <App />
                </HistoryRouter>
            );
            await expectHome();
            expect(history.index).toBe(0);
        });
    });

    describe('Displays data on transactions', () => {
        it('when transaction was a success', async () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText(successTransactionID);
            await expectTransactionStatus('success');
            expect(
                await screen.findByText('Transaction ID')
            ).toBeInTheDocument();
            expect(await screen.findByText('From')).toBeInTheDocument();
            expect(await screen.findByText('Event')).toBeInTheDocument();
            expect(await screen.findByText('Object')).toBeInTheDocument();
            expect(await screen.findByText('To')).toBeInTheDocument();
        });
        it('when transaction was a failure', () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText(failTransactionID);
            expectTransactionStatus('fail');
        });

        it('when transaction was pending', () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText(pendingTransactionID);
            expectTransactionStatus('pending');
        });

        it('when transaction data has missing info', () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText(problemTransactionID);
            expect(
                screen.getByText(
                    'There was an issue with the data on the following transaction'
                )
            ).toBeInTheDocument();
        });
    });

    describe('Displays data on objects', () => {
        it('when object was a success', async () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText(successObjectID);
            await checkObjectId(successObjectID);
            const el1 = screen.getByText('Object ID');
            expect(el1).toBeInTheDocument();
        });

        it('when object is read only', async () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText(readOnlyObject);
            await expectReadOnlyStatus('True');
        });

        it('when object is not read only', async () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText(notReadOnlyObject);
            await expectReadOnlyStatus('False');
        });

        it('when object data has missing info', async () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText(problemObjectID);
            expect(
                await screen.findByText(
                    'There was an issue with the data on the following object'
                )
            ).toBeInTheDocument();
        });
    });

    describe('Displays data on addresses', () => {
        it('when address has required fields', () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText(addressID);
            checkAddressId(addressID);
            const el1 = screen.getByText('Address ID');
            const el2 = screen.getByText('Owned Objects');
            expect(el1).toBeInTheDocument();
            expect(el2).toBeInTheDocument();
        });
        it('when address has missing fields', async () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText(problemAddressID);
            expect(
                await screen.findByText(
                    'No objects were found for the queried address value'
                )
            ).toBeInTheDocument();
        });
        it('when address has no objects', () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText(addressNoObjectsID);

            const el2 = screen.getByText(
                'No objects were found for the queried address value'
            );

            expect(el2).toBeInTheDocument();
        });
    });
    describe('Enables clicking links to', () => {
        it('go from address to object and back', async () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText(addressID);
            const el1 = await screen.findByText('playerOne');
            fireEvent.click(el1);

            await checkObjectId('playerOne');

            const el2 = await screen.findByText(addressID);

            fireEvent.click(el2);

            //await checkAddressId(addressID);
        });
        /*
        it('go from object to child object and back', async () => {
            const parentObj = 'playerTwo';
            const childObj = 'standaloneObject';

            render(<App />, { wrapper: MemoryRouter });
            searchText(parentObj);
            await checkObjectId(parentObj);

            const el1 = await screen.findByText(childObj);
            fireEvent.click(el1);
            await checkObjectId(childObj);

            const el2 = await screen.findByText(parentObj);
            fireEvent.click(el2);
            await checkObjectId(parentObj);
        });
        */
    });

    it('handles an ID with no associated data point', () => {
        render(<App />, { wrapper: MemoryRouter });
        searchText(noDataID);
        expect(
            screen.getByText('Data on the following query could not be found')
        ).toBeInTheDocument();
    });

    describe('Returns Home', () => {
        it('when Home Button is clicked', async () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText('Mysten Labs');
            fireEvent.click(screen.getByRole('link', { name: /home button/i }));
            await expectHome();
        });
    });
});
