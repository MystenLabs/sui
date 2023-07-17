// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';
import { useState } from 'react';

import { NetworkSelect, type NetworkSelectProps } from '~/ui/header/NetworkSelect';

export default {
	component: NetworkSelect,
	decorators: [
		(Story) => (
			<div className="flex justify-end bg-headerNav p-6">
				<Story />
			</div>
		),
	],
} as Meta;

const NETWORKS = [
	{ id: 'DEVNET', label: 'Devnet' },
	{ id: 'TESTNET', label: 'Testnet' },
	{ id: 'LOCAL', label: 'Local' },
];

export const Default: StoryObj<NetworkSelectProps> = {
	render: (args) => {
		const [network, setNetwork] = useState(NETWORKS[0].id);

		return <NetworkSelect {...args} value={network} version="1" onChange={setNetwork} />;
	},
	args: {
		networks: NETWORKS,
	},
};
