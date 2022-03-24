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

const expectReadOnlyStatus = (result: 'True' | 'False') => {
    const el = screen.getByTestId('read-only-text');
    expect(el).toHaveTextContent(result);
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
        it('when object was a success', () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText(successObjectID);
            expect(screen.getByText('Object ID')).toBeInTheDocument();
        });

        it('when object is read only', () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText(readOnlyObject);
            expectReadOnlyStatus('True');
        });

        it('when object is not read only', () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText(notReadOnlyObject);
            expectReadOnlyStatus('False');
        });

        it('when object data has missing info', () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText(problemObjectID);
            expect(
                screen.getByText(
                    'There was an issue with the data on the following object'
                )
            ).toBeInTheDocument();
        });
    });

    describe('Displays data on addresses', () => {
        it('when address has required fields', () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText(addressID);
            expect(screen.getByText('Address ID')).toBeInTheDocument();
            expect(screen.getByText('Owned Objects')).toBeInTheDocument();
        });
        it('when address has missing fields', () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText(problemAddressID);
            expect(
                screen.getByText(
                    'There was an issue with the data on the following address'
                )
            ).toBeInTheDocument();
        });
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
