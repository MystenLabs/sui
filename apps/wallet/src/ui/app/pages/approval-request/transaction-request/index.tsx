// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// import { Transaction } from '@mysten/sui.js';
import { Transaction } from '@mysten/sui.js';
import { useCallback, useMemo } from 'react';
import toast from 'react-hot-toast';

import { GasFees } from './GasFees';
import { TransactionDetails } from './TransactionDetails';
import { UserApproveContainer } from '_components/user-approve-container';
import { useAppDispatch } from '_hooks';
import { type TransactionApprovalRequest } from '_payloads/transactions/ApprovalRequest';
import { respondToTransactionRequest } from '_redux/slices/transaction-requests';
import { useSuiLedgerClient } from '_src/ui/app/components/ledger/SuiLedgerClientProvider';
import { useAccounts } from '_src/ui/app/hooks/useAccounts';
import { PageMainLayoutTitle } from '_src/ui/app/shared/page-main-layout/PageMainLayoutTitle';

import st from './TransactionRequest.module.scss';

export type TransactionRequestProps = {
    txRequest: TransactionApprovalRequest;
};

export function TransactionRequest({ txRequest }: TransactionRequestProps) {
    const accounts = useAccounts();
    const accountForTransaction = accounts.find(
        (account) => account.address === txRequest.tx.account
    );
    const { initializeLedgerSignerInstance } = useSuiLedgerClient();
    const dispatch = useAppDispatch();
    const transaction = useMemo(() => {
        const tx = Transaction.from(txRequest.tx.data);
        if (accountForTransaction) {
            tx.setSenderIfNotSet(accountForTransaction.address);
        }
        return tx;
    }, [txRequest.tx.data, accountForTransaction]);
    const addressForTransaction = txRequest.tx.account;
    const handleOnSubmit = useCallback(
        async (approved: boolean) => {
            if (accountForTransaction) {
                await dispatch(
                    respondToTransactionRequest({
                        approved,
                        txRequestID: txRequest.id,
                        accountForTransaction,
                        initializeLedgerSignerInstance,
                    })
                );
            } else {
                toast.error(
                    `Account for address ${txRequest.tx.account} not found`
                );
            }
        },
        [
            accountForTransaction,
            dispatch,
            txRequest.id,
            txRequest.tx.account,
            initializeLedgerSignerInstance,
        ]
    );

    return (
        <UserApproveContainer
            origin={txRequest.origin}
            originFavIcon={txRequest.originFavIcon}
            approveTitle="Approve"
            rejectTitle="Reject"
            onSubmit={handleOnSubmit}
            address={addressForTransaction}
        >
            <PageMainLayoutTitle title="Approve Transaction" />
            <section className={st.txInfo}>
                {/* MUSTFIX(chris) */}
                {/* <TransactionSummaryCard
                    transaction={tx}
                    address={addressForTransaction}
                /> */}
                <GasFees
                    sender={accountForTransaction?.address}
                    transaction={transaction}
                />
                <TransactionDetails
                    sender={accountForTransaction?.address}
                    transaction={transaction}
                />
            </section>
        </UserApproveContainer>
    );
}
