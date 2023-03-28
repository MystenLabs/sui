// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type SuiTransaction } from '@mysten/sui.js';

import { Command } from './Command';

import { TableHeader } from '~/ui/TableHeader';

interface Props {
    transactions: SuiTransaction[];
}

export function Commands({ transactions }: Props) {
    if (!transactions?.length) {
        return null;
    }

    return (
        <>
            <TableHeader>Commands</TableHeader>
            <ul className="flex flex-col gap-8">
                {transactions.map((command, index) => {
                    const commandName = Object.keys(command)[0];
                    const commandData =
                        command[commandName as keyof typeof command];

                    return (
                        <li key={index}>
                            <Command type={commandName} data={commandData} />
                        </li>
                    );
                })}
            </ul>
        </>
    );
}
