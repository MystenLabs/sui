// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { CoinBalance, type CoinBalanceProps } from '../CoinBalance';

export default {
    component: CoinBalance,
} as Meta;

export const Default: StoryObj<CoinBalanceProps> = {
    args: {
        amount: 1000,
        symbol: 'SUI',
    },
};

export const WithoutSymbol: StoryObj<CoinBalanceProps> = {
    args: {
        amount: 1000,
    },
};
