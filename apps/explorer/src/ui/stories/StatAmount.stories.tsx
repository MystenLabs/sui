// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { StatAmount, type StatAmountProps } from '../StatAmount';

export default {
    component: StatAmount,
    decorators: [
        (Story) => (
            <QueryClientProvider client={new QueryClient()}>
                <Story />
            </QueryClientProvider>
        ),
    ],
} as Meta;

export const defaultAmount: StoryObj<StatAmountProps> = {
    args: {
        amount: 9740991,
        currency: 'SUI',
        dollarAmount: 123.56,
        date: 1667942429177,
    },
};
