// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { type Meta, type StoryObj } from '@storybook/react';

import { StatAmount, type StatAmountProps } from '../StatAmount';

export default {
    component: StatAmount,
} as Meta;

const data = {
    amount: 9007199254740991n,
    currency: SUI_TYPE_ARG,
    dollarAmount: 123.56,
    date: 'June 24, 2012, 2:34 PM',
    
};

export const Default: StoryObj<StatAmountProps> = {
    render: () => <StatAmount {...data} />,
};
