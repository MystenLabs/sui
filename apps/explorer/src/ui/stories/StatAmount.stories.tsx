// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { StatAmount, type StatAmountProps } from '../StatAmount';

export default {
    component: StatAmount,
} as Meta;

const data = {
    amount: 9740991,
    currency: 'SUI',
    dollarAmount: 123.56,
    date: Date.now(),
};

export const Default: StoryObj<StatAmountProps> = {
    render: () => <StatAmount {...data} />,
};
