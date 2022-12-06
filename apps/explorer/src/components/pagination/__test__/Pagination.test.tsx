// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi } from 'vitest';

import Pagination from '../Pagination';

describe('Pagination', () => {
    it('check pagination buttons exist and check disabled behavior for the nextBtn and prevBtn', () => {
        const { getByTestId } = render(
            <Pagination totalItems={105} itemsPerPage={10} currentPage={1} />
        );

        const nextBtn = screen.getByRole('button', {
            name: /back/i,
        });

        const prevBtn = screen.getByRole('button', {
            name: /next/i,
        });

        const secondBtn = getByTestId('secondBtn');
        const secondLastBtn = getByTestId('secondLastBtn');

        expect(prevBtn).toBeDefined();
        expect(nextBtn).toBeDefined();
        expect(secondBtn).toBeDefined();
        expect(secondLastBtn).toBeDefined();
        expect(nextBtn).toHaveProperty('disabled', true);
        expect(prevBtn).not.toHaveProperty('disabled', true);
    });

    it('check active state for backBtn and nextBtn on the last page', () => {
        render(
            <Pagination totalItems={105} itemsPerPage={10} currentPage={11} />
        );

        const nextBtn = screen.getByRole('button', {
            name: /back/i,
        });

        const prevBtn = screen.getByRole('button', {
            name: /next/i,
        });

        expect(nextBtn).not.toHaveProperty('disabled', true);
        expect(prevBtn).toHaveProperty('disabled', true);
    });

    it('check pagination button values', () => {
        render(
            <Pagination totalItems={105} itemsPerPage={10} currentPage={1} />
        );

        // Handle multiple elements with the same test id:
        const firstBtn = screen.getByRole('button', {
            name: /first page/i,
        });

        const lastBtn = screen.getByRole('button', {
            name: /last page/i,
        });

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

        const lastBtn = screen.getByRole('button', {
            name: /last page/i,
        });

        const lastBtnPageNum = parseInt(lastBtn.textContent || '0', 10);

        await userEvent.click(secondBtn);
        expect(setPageNumChange).toHaveReturnedWith(2);

        await userEvent.click(secondBtn);
        expect(setPageNumChange).toHaveReturnedWith(secondBtnPageNum);

        await userEvent.click(secondLastBtn);
        expect(setPageNumChange).toHaveReturnedWith(secondLastBtnNumber);

        await userEvent.click(lastBtn);
        expect(setPageNumChange).toHaveReturnedWith(lastBtnPageNum);
    });
});
