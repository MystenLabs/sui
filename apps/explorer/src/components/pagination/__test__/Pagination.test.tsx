// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { render } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi } from 'vitest';

import Pagination from '../Pagination';

describe('Pagination test', () => {
    it('check pagination buttons exist and check disabled behavior for the nextBtn and prevBtn', () => {
        const { getByTestId, container } = render(
            <Pagination totalItems={105} itemsPerPage={10} currentPage={1} />
        );
        const nextBtn = container.querySelector(
            'button[data-testid="nextBtn"]:not([style*="display:none"])'
        );
        const prevBtn = container.querySelector(
            'button[data-testid="backBtn"]:not([style*="display:none"])'
        );
        const secondBtn = getByTestId('secondBtn');
        const secondLastBtn = getByTestId('secondLastBtn');

        expect(prevBtn).toBeDefined();
        expect(nextBtn).toBeDefined();
        expect(secondBtn).toBeDefined();
        expect(secondLastBtn).toBeDefined();
        expect(nextBtn).not.toHaveProperty('disabled', true);
        expect(prevBtn).toHaveProperty('disabled', true);
    });

    it('check active state for backBtn and nextBtn on the last page', () => {
        const { container } = render(
            <Pagination totalItems={105} itemsPerPage={10} currentPage={11} />
        );

        const nextBtn = container.querySelector(
            'button[data-testid="nextBtn"]:not([style*="display:none"])'
        );
        const prevBtn = container.querySelector(
            'button[data-testid="backBtn"]:not([style*="display:none"])'
        );
        expect(nextBtn).toHaveProperty('disabled', true);
        expect(prevBtn).not.toHaveProperty('disabled', true);
    });

    it('check pagination button values', () => {
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

    it('onPageChangeFn should match clicked button page number', async () => {
        const setPageNumChange = vi.fn((number) => number);

        const { getByTestId } = render(
            <Pagination
                totalItems={105}
                itemsPerPage={10}
                currentPage={1}
                onPagiChangeFn={setPageNumChange}
            />
        );

        const secondBtn = getByTestId('secondBtn');
        const secondBtnPageNum = parseInt(secondBtn.textContent || '0', 10);
        const secondLastBtn = getByTestId('secondLastBtn');
        const secondLastBtnNumber = parseInt(
            secondLastBtn?.textContent || '0',
            10
        );

        await userEvent.click(secondBtn);
        expect(setPageNumChange).toHaveReturnedWith(2);

        await userEvent.click(secondBtn);
        expect(setPageNumChange).toHaveReturnedWith(secondBtnPageNum);

        await userEvent.click(secondLastBtn);
        expect(setPageNumChange).toHaveReturnedWith(secondLastBtnNumber);
    });
});
