import { render, screen, fireEvent } from '@testing-library/react';
import { createMemoryHistory } from 'history';
import {
    MemoryRouter,
    unstable_HistoryRouter as HistoryRouter,
} from 'react-router-dom';

import App from './App';

function expectHome() {
    expect(screen.getByText(/This is home page/i)).toBeInTheDocument();
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
        fireEvent.click(screen.getByText('#tx1'));
        expect(screen.getByText('Transaction #tx1')).toBeInTheDocument();
    });
    it('redirects to search result', () => {
        render(<App />, { wrapper: MemoryRouter });
        fireEvent.click(screen.getByText('aTerm'));
        expect(
            screen.getByText('Search results for "aTerm"')
        ).toBeInTheDocument();
    });
});
