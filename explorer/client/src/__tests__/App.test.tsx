import { render, screen, fireEvent } from '@testing-library/react';
import { createMemoryHistory } from 'history';
import {
    MemoryRouter,
    unstable_HistoryRouter as HistoryRouter,
} from 'react-router-dom';

import App from '../app/App';

function expectHome() {
    expect(screen.getByText(/Latest Transactions/i)).toBeInTheDocument();
}

function searchText(text: string) {
    fireEvent.change(
        screen.getByPlaceholderText(/Search transactions by ID/i),
        { target: { value: text } }
    );
    fireEvent.submit(screen.getByRole('form', { name: /search form/i }));
}

function expectTransactionStatus(result: 'fail' | 'success') {
    expect(screen.getByTestId('transaction-status')).toHaveTextContent(result);
}

describe('App component', () => {
    it('renders the home page', () => {
        render(<App />, { wrapper: MemoryRouter });
        expectHome();
    });
    it('redirects to home for every unknown path', () => {
        render(
            <MemoryRouter initialEntries={['/anything']}>
                <App />
            </MemoryRouter>
        );
        expectHome();
    });
    it('redirects to home for unknown path by replacing the history', () => {
        const history = createMemoryHistory({ initialEntries: ['/anything'] });
        render(
            <HistoryRouter history={history}>
                <App />
            </HistoryRouter>
        );
        expectHome();
        expect(history.index).toBe(0);
    });
    it('redirects to transaction details', () => {
        render(<App />, { wrapper: MemoryRouter });
        searchText(
            'A1dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd'
        );
        expect(screen.getByText('Transaction ID')).toBeInTheDocument();
        expectTransactionStatus('success');
    });
    it('complains when transaction cannot be found', () => {
        render(<App />, { wrapper: MemoryRouter });
        searchText(
            'A1ddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddde'
        );
        expect(
            screen.getByText('This transaction could not be found:')
        ).toBeInTheDocument();
    });
    it('details a transaction failure', () => {
        render(<App />, { wrapper: MemoryRouter });
        searchText(
            'A2dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd'
        );
        expectTransactionStatus('fail');
    });

    it('redirects to search result', () => {
        render(<App />, { wrapper: MemoryRouter });
        searchText('Mysten Labs');
        expect(
            screen.getByText('Search results for "Mysten Labs"')
        ).toBeInTheDocument();
    });
    it('returns home when Home is clicked', () => {
        render(<App />, { wrapper: MemoryRouter });
        searchText('Mysten Labs');
        fireEvent.click(screen.getByRole('link', { name: /home button/i }));
        expectHome();
    });
    it('returns home when Title Logo is clicked', () => {
        render(<App />, { wrapper: MemoryRouter });
        searchText('Mysten Labs');
        fireEvent.click(screen.getByRole('link', { name: /logo/i }));
        expectHome();
    });

    describe('latest transactions', () => {
        it('shows latest transactions on home page', () => {
            render(<App />, { wrapper: MemoryRouter });
            expect(screen.getByText('Transaction ID')).toBeInTheDocument();
        });
        it('navigates to transaction details when clicked', () => {
            const history = createMemoryHistory();
            render(
                <HistoryRouter history={history}>
                    <App />
                </HistoryRouter>
            );
            fireEvent.click(screen.getByText(/A1dddd/));
            expect(history.location.pathname).toBe(
                '/transactions/A1dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd'
            );
        });
    });
});
