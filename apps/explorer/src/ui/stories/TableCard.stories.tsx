// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type StoryObj, type Meta } from '@storybook/react';
import { type ReactNode } from 'react';

import { TableCard, type TableCardProps } from '../TableCard';

type DataType = {
	sardines: string | ReactNode;
	herrings: string | ReactNode;
	salmon: string;
};

const data = {
	data: [
		{
			sardines: (
				<div>
					Some custom HTML:
					<ul>
						<li>
							<i>Sardina pilchardus</i>
						</li>
						<li>
							<i>Engraulis ringens</i>
						</li>
					</ul>
				</div>
			),
			herrings: (
				<div>
					The below has a hover effect:{' '}
					<ul>
						<li>
							<i className="hover:text-red-900 cursor-pointer">Clupea harengus</i>
						</li>
					</ul>
				</div>
			),
			salmon: 'This is plain text but the column heading is emphasised',
		},
		{
			sardines: 'second row cell can have different content',
			herrings: 'this is plain text',
			salmon: 'This is also plain text',
		},
	],
	columns: [
		{
			header: 'Sardines',
			accessorKey: 'sardines',
		},
		{
			header: 'Herrings',
			accessorKey: 'herrings',
		},
		{
			header: () => <i>Salmon</i>,
			accessorKey: 'salmon',
		},
	],
};

export default {
	component: TableCard,
} as Meta;

export const VaryingWidth: StoryObj<TableCardProps<DataType>> = {
	render: () => <TableCard data={data.data} columns={data.columns} />,
};
