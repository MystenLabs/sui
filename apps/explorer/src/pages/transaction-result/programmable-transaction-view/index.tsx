// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type ProgrammableTransaction } from '@mysten/sui.js';

import { Inputs } from './Inputs';
import { Transactions } from './Transactions';

interface Props {
    transaction: ProgrammableTransaction;
}

export function ProgrammableTransactionView({ transaction }: Props) {
    return (
        <>
            <section className="pt-12">
                <Inputs inputs={transaction.inputs} />
            </section>

<<<<<<< HEAD
            <section className="py-12">
                <Transactions transactions={transaction.transactions} />
=======
            <section
                data-testid="programmable-transactions-commands"
                className="py-12"
            >
                <Commands transactions={transaction.transactions} />
>>>>>>> fork/testnet
            </section>
        </>
    );
}
