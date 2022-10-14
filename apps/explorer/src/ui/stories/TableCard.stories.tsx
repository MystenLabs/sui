// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ComponentStory, type ComponentMeta } from '@storybook/react';

import { TableCard } from '../TableCard';

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
                            <i className="hover:text-red-900 cursor-pointer">
                                Clupea harengus
                            </i>
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
            headerLabel: 'Sardines',
            accessorKey: 'sardines',
        },
        {
            headerLabel: 'Herrings',
            accessorKey: 'herrings',
        },
        {
            headerLabel: () => <i>Salmon</i>,
            accessorKey: 'salmon',
        },
    ],
};

export default {
    title: 'UI/TableCard',
    component: TableCard,
} as ComponentMeta<typeof TableCard>;

export const VaryingWidth: ComponentStory<typeof TableCard> = (args) => (
    <TableCard tabledata={data} />
);
