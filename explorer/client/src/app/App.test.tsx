import { render, screen } from '@testing-library/react';

import App from './App';

describe('App component', () => {
    it('renders the app', () => {
        render(<App />);
        const linkElement = screen.getByText(/no transactions here yet/i);
        expect(linkElement).toBeInTheDocument();
    });
});
