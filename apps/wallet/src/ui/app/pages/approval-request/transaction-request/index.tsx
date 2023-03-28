// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// import { Transaction } from '@mysten/sui.js';
import { TransactionBlock } from '@mysten/sui.js';
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
    const addressForTransaction = txRequest.tx.account;
    const signer = useSigner(addressForTransaction);
    const dispatch = useAppDispatch();
    const transaction = useMemo(() => {
        const tx = TransactionBlock.from(txRequest.tx.data);
        if (addressForTransaction) {
            tx.setSenderIfNotSet(addressForTransaction);
        }
        return tx;
    }, [txRequest.tx.data, addressForTransaction]);

    const handleOnSubmit = useCallback(
        async (approved: boolean) => {
            await dispatch(
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
                    sender={addressForTransaction}
                    transaction={transaction}
                />
                <TransactionDetails
                    sender={addressForTransaction}
                    transaction={transaction}
                />
            </section>
        </UserApproveContainer>
    );
}
