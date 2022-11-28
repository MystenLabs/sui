// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { render } from '@testing-library/react';
import { describe, it, expect } from 'vitest';

import Pagination from '../Pagination';

describe('Pagination unit test', () => {
    it('check pagination buttons', () => {
        const { getByRole, getByTestId } = render(
            <Pagination totalItems={105} itemsPerPage={10} currentPage={1} />
        );
        const nextBtn = getByRole('button', { name: /→/i });
        const prevBtn = getByRole('button', { name: /←/i });
        const secondBtn = getByTestId('secondBtn');
        const secondLastBtn = getByTestId('secondLastBtn');

        expect(prevBtn).toBeDefined();
        expect(nextBtn).toBeDefined();
        expect(secondBtn).toBeDefined();
        expect(secondLastBtn).toBeDefined();
        expect(nextBtn).not.toHaveProperty('disabled', true);
        expect(prevBtn).toHaveProperty('disabled', true);
    });

    it('check button values for last page', () => {
        const { getByRole } = render(
            <Pagination totalItems={105} itemsPerPage={10} currentPage={11} />
        );
        const nextBtn = getByRole('button', { name: /→/i });
        const prevBtn = getByRole('button', { name: /←/i });

        expect(nextBtn).toHaveProperty('disabled', true);
        expect(prevBtn).not.toHaveProperty('disabled', true);
    });

    it('check pagination values', () => {
        const { container } = render(
            <Pagination totalItems={105} itemsPerPage={10} currentPage={1} />
        );

        // Handle multiple elements with the same test id:
        const firstBtn = container.querySelector(
            'button[data-testid="firstBtn"]:not([style*="display:none"])'
        );
        const lastBtn = container.querySelector(
            'button[data-testid="lastBtn"]:not([style*="display:none"])'
        );

        expect(firstBtn).toBeDefined();
        expect(lastBtn).toBeDefined();
        expect(firstBtn?.textContent).toBe('1');
        expect(lastBtn?.textContent).toBe('11');
    });
});
