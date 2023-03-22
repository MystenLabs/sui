// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type ProgrammableTransaction } from '@mysten/sui.js';

import { Commands } from './Commands';
import { Inputs } from './Inputs';

interface Props {
    transaction: ProgrammableTransaction;
}

export function ProgrammableTransactionView({ transaction }: Props) {
    return (
        <>
            <section
                data-testid="programmable-transactions-inputs"
                className="pt-12"
            >
                <Inputs inputs={transaction.inputs} />
            </section>

            <section
                data-testid="programmable-transactions-commands"
                className="py-12"
            >
                <Commands commands={transaction.commands} />
            </section>
        </>
    );
}
