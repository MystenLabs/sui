// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { TransactionType, type TransactionTypeProps } from '../TransactionType';

export default {
    component: TransactionType,
} as Meta;

export const Default: StoryObj<TransactionTypeProps> = {
    render: () => (
        <div className="flex gap-[21px]">
            <TransactionType isSuccess>Call</TransactionType>
            <TransactionType>Call</TransactionType>
            <TransactionType isSuccess>TransferObject</TransactionType>
            <TransactionType isSuccess>ChangeEpoch</TransactionType>
            <TransactionType isSuccess count="42">
                Batch
            </TransactionType>
        </div>
    ),
};
