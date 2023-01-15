// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useQuery } from '@tanstack/react-query';
import { memo, useMemo } from 'react';

import PageTitle from '_app/shared/page-title';
import RecentTransactions from '_components/transactions-card/RecentTransactions';
import { TxnItem } from '_components/transactions-card/Transaction';
import { useAppSelector, useRpc } from '_hooks';

import st from './Transactions.module.scss';
// Remove duplicate transactionsId, reduces the number of RPC calls
const dedupe = (results: string[] | undefined) =>
    results
        ? results.filter((value, index, self) => self.indexOf(value) === index)
        : [];

function TransactionsPage() {
    // get address
    // Get txn data
    // get batch of txns
    const rpc = useRpc();
    const activeAddress = useAppSelector(({ account: { address } }) => address);

    const {
        data: txnsIds,
        isError: sErrorTxnsId,
        isLoading: isLoadingTxnsIds,
    } = useQuery(
        ['txns', activeAddress],
        () => rpc.getTransactionsForAddress(activeAddress || '', true),
        { enabled: !!activeAddress }
    );

    const dedupedTxnsIds = useMemo(
        () => (txnsIds ? dedupe(txnsIds) : []),
        [txnsIds]
    );

    const {
        data: txns,
        isError: isErrorTxns,
        isLoading: isLoadingTxns,
    } = useQuery(
        ['txns', 'allTxns'],
        async () => rpc.getTransactionWithEffectsBatch(dedupedTxnsIds),
        { enabled: !!dedupedTxnsIds.length }
    );

    return (
        <div className="flex flex-col flex-nowrap h-full">
            <PageTitle
                title="Your Activity"
                className="flex justify-center text-heading6 text-gray-90"
            />

            <div className={st.txContent}>
                {txns &&
                    txns.map((txn) => (
                        <TxnItem
                            key={txn.certificate.transactionDigest}
                            txn={txn}
                        />
                    ))}
                <RecentTransactions />
            </div>
        </div>
    );
}

export default memo(TransactionsPage);
