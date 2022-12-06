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
            <TxType isSuccess>Call</TxType>
            <TxType>Call</TxType>
            <TxType isSuccess>TransferObject</TxType>
            <TxType isSuccess>ChangeEpoch</TxType>
            <TxType isSuccess count="42">
                Batch
            </TxType>
        </div>
    ),
};
