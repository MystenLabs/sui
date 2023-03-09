// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui.js';
import { useCallback, useMemo } from 'react';

import { Permissions } from './Permissions';
import { SummaryCard } from './SummaryCard';
import { TransactionSummaryCard } from './TransactionSummaryCard';
import { TransactionTypeCard } from './TransactionTypeCard';
import { UserApproveContainer } from '_components/user-approve-container';
import { useAppDispatch } from '_hooks';
import { type TransactionApprovalRequest } from '_payloads/transactions/ApprovalRequest';
import { respondToTransactionRequest } from '_redux/slices/transaction-requests';

import st from './TransactionRequest.module.scss';

interface MetadataGroup {
    name: string;
    children: { id: string; module: string }[];
}

export type TransactionRequestProps = {
    txRequest: TransactionApprovalRequest;
};

export function TransactionRequest({ txRequest }: TransactionRequestProps) {
    const dispatch = useAppDispatch();
    const tx = useMemo(
        () => Transaction.from(txRequest.tx.data),
        [txRequest.tx.data]
    );
    const addressForTransaction = txRequest.tx.account;
    const handleOnSubmit = useCallback(
        async (approved: boolean) => {
            await dispatch(
                respondToTransactionRequest({
                    approved,
                    txRequestID: txRequest.id,
                    addressForTransaction,
                })
            );
        },
        [dispatch, txRequest, addressForTransaction]
    );

    // TODO: Add back metadata support:
    const metadata = useMemo(() => {
        const transfer: MetadataGroup = { name: 'Transfer', children: [] };
        const modify: MetadataGroup = { name: 'Modify', children: [] };
        const read: MetadataGroup = { name: 'Read', children: [] };

        // TODO: Update this metadata:
        // txRequest.metadata.parameters.forEach((param, index) => {
        //     if (typeof param !== 'object') return;
        //     const id = txData?.arguments?.[index] as string;
        //     if (!id) return;

        //     // TODO: Support non-flat arguments.
        //     if (typeof id !== 'string') return;

        //     const unwrappedType = unwrapTypeReference(param);
        //     if (!unwrappedType) return;

        //     const groupedParam = {
        //         id,
        //         module: `${unwrappedType.address}::${unwrappedType.module}::${unwrappedType.name}`,
        //     };

        //     if ('Struct' in param) {
        //         transfer.children.push(groupedParam);
        //     } else if ('MutableReference' in param) {
        //         // Skip TxContext:
        //         if (groupedParam.module === TX_CONTEXT_TYPE) return;
        //         modify.children.push(groupedParam);
        //     } else if ('Reference' in param) {
        //         read.children.push(groupedParam);
        //     }
        // });

        // if (
        //     !transfer.children.length &&
        //     !modify.children.length &&
        //     !read.children.length
        // ) {
        //     return null;
        // }

        return {
            transfer,
            modify,
            read,
        };
    }, []);

    const valuesContent: {
        label: string;
        content: string | number | null;
        loading?: boolean;
    }[] = useMemo(() => {
        // TODO: Support metadata:
        return [
            // {
            //     label: 'Transaction Type',
            //     content: txRequest.tx.data.kind,
            // },
            // {
            //     label: 'Function',
            //     content: moveCallTxn.function,
            // },
            // {
            //     label: 'Module',
            //     content: moveCallTxn.module,
            // },
        ];
    }, []);

    return (
        <UserApproveContainer
            origin={txRequest.origin}
            originFavIcon={txRequest.originFavIcon}
            approveTitle="Approve"
            rejectTitle="Reject"
            onSubmit={handleOnSubmit}
            address={addressForTransaction}
        >
            <section className={st.txInfo}>
                <TransactionSummaryCard
                    transaction={tx}
                    address={addressForTransaction}
                />
                <Permissions metadata={metadata} />
                <SummaryCard
                    transparentHeader
                    header={
                        <>
                            <div className="font-medium text-sui-steel-darker">
                                Transaction Type
                            </div>
                            {/* <div className="font-semibold text-sui-steel-darker">
                                {valuesContent[0].content}
                            </div> */}
                        </>
                    }
                >
                    <div className={st.content}>
                        {valuesContent
                            .slice(1)
                            .map(({ label, content, loading = false }) => (
                                <div key={label} className={st.row}>
                                    <TransactionTypeCard
                                        label={label}
                                        content={content}
                                        loading={loading}
                                    />
                                </div>
                            ))}
                    </div>
                </SummaryCard>
            </section>
        </UserApproveContainer>
    );
}
