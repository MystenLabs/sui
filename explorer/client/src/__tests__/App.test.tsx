// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { render, screen, fireEvent } from '@testing-library/react';
import { createMemoryHistory } from 'history';
import {
    MemoryRouter,
    unstable_HistoryRouter as HistoryRouter,
} from 'react-router-dom';

import App from '../app/App';

function expectHome() {
    expect(screen.getByTestId('home-page')).toBeInTheDocument();
}

function searchText(text: string) {
    fireEvent.change(screen.getByPlaceholderText(/Search by ID/i), {
        target: { value: text },
    });
    fireEvent.submit(screen.getByRole('form', { name: /search form/i }));
}

describe('End-to-end Tests', () => {
    it('renders the home page', () => {
        render(<App />, { wrapper: MemoryRouter });
        expectHome();
    });

    describe('Redirects to Home Page', () => {
        it('redirects to home for every unknown path', () => {
            render(
                <MemoryRouter initialEntries={['/anything']}>
                    <App />
                </MemoryRouter>
            );
            expectHome();
        });
        it('redirects to home for unknown path by replacing the history', () => {
            const history = createMemoryHistory({
                initialEntries: ['/anything'],
            });
            render(
                <HistoryRouter history={history}>
                    <App />
                </HistoryRouter>
            );
            expectHome();
            expect(history.index).toBe(0);
        });
    });

    describe('Returns Home', () => {
        it('when Home Button is clicked', () => {
            render(<App />, { wrapper: MemoryRouter });
            searchText('Mysten Labs');
            fireEvent.click(screen.getByRole('link', { name: /home button/i }));
            expectHome();
        });
    });
});
