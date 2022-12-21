// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useMemo, useState } from 'react';
import { useParams } from 'react-router-dom';

import { Permissions } from './Permissions';
import { SummaryCard } from './SummaryCard';
import { TransactionSummaryCard } from './TransactionSummaryCard';
import { TransactionTypeCard } from './TransactionTypeCard';
import Loading from '_components/loading';
import UserApproveContainer from '_components/user-approve-container';
import { useAppDispatch, useAppSelector } from '_hooks';
import {
    loadTransactionResponseMetadata,
    respondToTransactionRequest,
    txRequestsSelectors,
    deserializeTxn,
} from '_redux/slices/transaction-requests';

import type {
    SuiMoveNormalizedType,
    MoveCallTransaction,
} from '@mysten/sui.js';
import type { RootState } from '_redux/RootReducer';

import st from './DappTxApprovalPage.module.scss';

interface MetadataGroup {
    name: string;
    children: { id: string; module: string }[];
}

interface TypeReference {
    address: string;
    module: string;
    name: string;
    type_arguments: SuiMoveNormalizedType[];
}

const TX_CONTEXT_TYPE = '0x2::tx_context::TxContext';

/** Takes a normalized move type and returns the address information contained within it */
function unwrapTypeReference(
    type: SuiMoveNormalizedType
): null | TypeReference {
    if (typeof type === 'object') {
        if ('Struct' in type) {
            return type.Struct;
        }
        if ('Reference' in type) {
            return unwrapTypeReference(type.Reference);
        }
        if ('MutableReference' in type) {
            return unwrapTypeReference(type.MutableReference);
        }
        if ('Vector' in type) {
            return unwrapTypeReference(type.Vector);
        }
    }
    return null;
}

export function DappTxApprovalPage() {
    const { txID } = useParams();

    const [txRequestsLoading, deserializeTxnFailed] = useAppSelector(
        ({ transactionRequests }) => [
            !transactionRequests.initialized,
            transactionRequests.deserializeTxnFailed,
        ]
    );

    const txRequestSelector = useMemo(
        () => (state: RootState) =>
            (txID && txRequestsSelectors.selectById(state, txID)) || null,
        [txID]
    );

    const txRequest = useAppSelector(txRequestSelector);
    const loading = txRequestsLoading;
    const dispatch = useAppDispatch();
    const handleOnSubmit = useCallback(
        async (approved: boolean) => {
            if (txRequest) {
                await dispatch(
                    respondToTransactionRequest({
                        approved,
                        txRequestID: txRequest.id,
                    })
                );
            }
        },
        [dispatch, txRequest]
    );

    useEffect(() => {
        if (txRequest?.tx?.type === 'move-call' && !txRequest.metadata) {
            dispatch(
                loadTransactionResponseMetadata({
                    txRequestID: txRequest.id,
                    objectId: txRequest.tx.data.packageObjectId,
                    moduleName: txRequest.tx.data.module,
                    functionName: txRequest.tx.data.function,
                })
            );
        }

        if (txRequest?.tx?.type === 'v2' && !txRequest.metadata) {
            const reqData = txRequest.tx.data.data as MoveCallTransaction;
            dispatch(
                loadTransactionResponseMetadata({
                    txRequestID: txRequest.id,
                    objectId: reqData.packageObjectId,
                    moduleName: reqData.module,
                    functionName: reqData.function,
                })
            );
        }

        if (
            txRequest?.tx?.type === 'serialized-move-call' &&
            !txRequest.unSerializedTxn &&
            txRequest?.tx.data
        ) {
            dispatch(
                deserializeTxn({
                    serializedTxn: txRequest?.tx.data,
                    id: txRequest.id,
                })
            );
        }
    }, [txRequest, dispatch]);

    const metadata = useMemo(() => {
        if (
            (txRequest?.tx?.type !== 'move-call' &&
                txRequest?.tx?.type !== 'v2' &&
                txRequest?.tx?.type !== 'serialized-move-call' &&
                !txRequest?.unSerializedTxn) ||
            !txRequest?.metadata
        ) {
            return null;
        }
        const moveTxData =
            txRequest?.tx?.type === 'v2'
                ? txRequest.tx.data.data
                : txRequest.tx.data;
        const txData =
            (txRequest?.unSerializedTxn?.data as MoveCallTransaction) ??
            moveTxData;

        const transfer: MetadataGroup = { name: 'Transfer', children: [] };
        const modify: MetadataGroup = { name: 'Modify', children: [] };
        const read: MetadataGroup = { name: 'Read', children: [] };

        txRequest.metadata.parameters.forEach((param, index) => {
            if (typeof param !== 'object') return;
            const id = txData?.arguments?.[index] as string;
            if (!id) return;

            const unwrappedType = unwrapTypeReference(param);
            if (!unwrappedType) return;

            const groupedParam = {
                id,
                module: `${unwrappedType.address}::${unwrappedType.module}::${unwrappedType.name}`,
            };

            if ('Struct' in param) {
                transfer.children.push(groupedParam);
            } else if ('MutableReference' in param) {
                // Skip TxContext:
                if (groupedParam.module === TX_CONTEXT_TYPE) return;
                modify.children.push(groupedParam);
            } else if ('Reference' in param) {
                read.children.push(groupedParam);
            }
        });

        if (
            !transfer.children.length &&
            !modify.children.length &&
            !read.children.length
        ) {
            return null;
        }

        return {
            transfer,
            modify,
            read,
        };
    }, [txRequest]);

    useEffect(() => {
        if (
            !loading &&
            (!txRequest || (txRequest && txRequest.approved !== null))
        ) {
            window.close();
        }
    }, [loading, txRequest]);

    // prevent serialized-move-call from being rendered while deserializing move-call
    const [loadingState, setLoadingState] = useState<boolean>(true);
    useEffect(() => {
        if (
            (!loading && txRequest?.tx.type !== 'serialized-move-call') ||
            (!loading &&
                txRequest?.tx.type === 'serialized-move-call' &&
                (txRequest?.metadata || deserializeTxnFailed))
        ) {
            setLoadingState(false);
        }
    }, [deserializeTxnFailed, loading, txRequest]);

    const valuesContent: {
        label: string;
        content: string | number | null;
        loading?: boolean;
    }[] = useMemo(() => {
        switch (txRequest?.tx.type) {
            case 'v2': {
                const data = txRequest.tx.data;
                return [
                    {
                        label: 'Transaction Type',
                        content: data.kind,
                    },
                ];
            }
            case 'move-call':
                return [
                    { label: 'Transaction Type', content: 'MoveCall' },
                    {
                        label: 'Function',
                        content: txRequest.tx.data.function,
                    },
                ];
            case 'serialized-move-call':
                return [
                    ...(txRequest?.unSerializedTxn
                        ? [
                              {
                                  label: 'Function',
                                  content:
                                      (
                                          txRequest?.unSerializedTxn
                                              ?.data as MoveCallTransaction
                                      )?.function.replace(/_/g, ' ') ?? '',
                              },
                              {
                                  label: 'Module',
                                  content:
                                      (
                                          txRequest?.unSerializedTxn
                                              ?.data as MoveCallTransaction
                                      )?.module.replace(/_/g, ' ') ?? '',
                              },
                          ]
                        : [
                              {
                                  label: 'Content',
                                  content: txRequest?.tx.data,
                              },
                          ]),
                ];
            default:
                return [];
        }
    }, [txRequest?.tx, txRequest?.unSerializedTxn]);

    const address = useAppSelector(({ account: { address } }) => address);

    return (
        <Loading loading={loadingState}>
            {txRequest ? (
                <UserApproveContainer
                    origin={txRequest.origin}
                    originFavIcon={txRequest.originFavIcon}
                    approveTitle="Approve"
                    rejectTitle="Reject"
                    onSubmit={handleOnSubmit}
                >
                    <section className={st.txInfo}>
                        {txRequest?.tx && address && (
                            <TransactionSummaryCard
                                txRequest={txRequest}
                                address={address}
                            />
                        )}
                        <Permissions metadata={metadata} />
                        <SummaryCard
                            transparentHeader
                            header={
                                <>
                                    <div className="font-medium text-sui-steel-darker">
                                        Transaction Type
                                    </div>
                                    <div className="font-semibold text-sui-steel-darker">
                                        {txRequest?.unSerializedTxn?.kind ??
                                            txRequest?.tx?.type}
                                    </div>
                                </>
                            }
                        >
                            <div className={st.content}>
                                {valuesContent.map(
                                    ({ label, content, loading = false }) => (
                                        <div key={label} className={st.row}>
                                            <TransactionTypeCard
                                                label={label}
                                                content={content}
                                                loading={loading}
                                            />
                                        </div>
                                    )
                                )}
                            </div>
                        </SummaryCard>
                    </section>
                </UserApproveContainer>
            ) : null}
        </Loading>
    );
}
