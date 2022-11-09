// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { Amount, type AmountProps } from '../Amount';

export default {
    component: Amount,
} as Meta;

export const SuiAmount: StoryObj<AmountProps> = {
    args: {
        amount: 1000,
        coinSymbol: 'SUI',
    },
};
