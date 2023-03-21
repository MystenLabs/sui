// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// import { Transaction } from '@mysten/sui.js';
import { Transaction } from '@mysten/sui.js';
import { useCallback, useMemo } from 'react';

import { GasFees } from './GasFees';
import { TransactionDetails } from './TransactionDetails';
import { UserApproveContainer } from '_components/user-approve-container';
import { useAppDispatch, useSigner } from '_hooks';
import { type TransactionApprovalRequest } from '_payloads/transactions/ApprovalRequest';
import { respondToTransactionRequest } from '_redux/slices/transaction-requests';
import { PageMainLayoutTitle } from '_src/ui/app/shared/page-main-layout/PageMainLayoutTitle';

import st from './TransactionRequest.module.scss';

export type TransactionRequestProps = {
    txRequest: TransactionApprovalRequest;
};

export function TransactionRequest({ txRequest }: TransactionRequestProps) {
    const signer = useSigner(txRequest.tx.account);
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
        (approved: boolean) => {
            dispatch(
                respondToTransactionRequest({
                    approved,
                    txRequestID: txRequest.id,
                    signer,
                })
            );
        },
        [dispatch, txRequest.id, signer]
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
