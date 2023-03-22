// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type ProgrammableTransactionCommand } from '@mysten/sui.js';

import { Command } from './Command';

import { TableHeader } from '~/ui/TableHeader';

interface Props {
    commands: ProgrammableTransactionCommand[];
}

export function Commands({ commands }: Props) {
    if (!commands?.length) {
        return null;
    }

    return (
        <>
            <TableHeader>Commands</TableHeader>
            <ul className="flex flex-col gap-8">
                {commands.map((command, index) => {
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
