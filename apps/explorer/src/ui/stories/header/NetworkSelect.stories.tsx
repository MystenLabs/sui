// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';
import { useState } from 'react';

import {
    NetworkSelect,
    type NetworkSelectProps,
} from '~/ui/header/NetworkSelect';

export default {
    component: NetworkSelect,
    decorators: [
        (Story) => (
            <div className="bg-headerNav p-6 flex justify-end">
                <Story />
            </div>
        ),
    ],
} as Meta;

export const Default: StoryObj<NetworkSelectProps> = {
    render: (args) => {
        const [network, setNetwork] = useState('Devnet');

        return (
            <NetworkSelect {...args} value={network} onChange={setNetwork} />
        );
    },
    args: {
        networks: ['Devnet', 'Testnet', 'Local'],
    },
};
