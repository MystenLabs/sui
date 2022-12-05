// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { TxType, type TxTypeProps } from '../TxType';

export default {
    component: TxType,
} as Meta;

export const Default: StoryObj<TxTypeProps> = {
    render: () => (
        <div className="flex gap-[21px]">
            <TxType>Call</TxType>
            <TxType isFail>Call</TxType>
            <TxType>TransferObject</TxType>
            <TxType>ChangeEpoch</TxType>
            <TxType count="42">Batch</TxType>
        </div>
    ),
};
