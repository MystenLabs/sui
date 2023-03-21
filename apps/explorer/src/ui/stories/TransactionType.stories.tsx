// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { TransactionType, type TransactionTypeProps } from '../TransactionType';

export default {
    component: TransactionType,
} as Meta;

export const Success: StoryObj<TransactionTypeProps> = {
    args: { isSuccess: true, type: 'ProgrammableTransaction' },
};

export const Fail: StoryObj<TransactionTypeProps> = {
    args: { type: 'ProgrammableTransaction' },
};

export const WithNumber: StoryObj<TransactionTypeProps> = {
    args: { isSuccess: true, type: 'Batch', count: '42' },
};
