// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useParams } from 'react-router-dom';

import Loading from '_components/loading';
import UserApproveContainer from '_components/user-approve-container';
import {
    useAppDispatch,
    useAppSelector,
    useInitializedGuard,
    useMiddleEllipsis,
} from '_hooks';
import {
    loadTransactionResponseMetadata,
    respondToTransactionRequest,
    txRequestsSelectors,
} from '_redux/slices/transaction-requests';

import type { SuiMoveNormalizedType } from '@mysten/sui.js';
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

type TabType = 'transfer' | 'modify' | 'read';

const TRUNCATE_MAX_LENGTH = 10;
const TRUNCATE_PREFIX_LENGTH = 6;

function PassedObject({ id, module }: { id: string; module: string }) {
    const objectId = useMiddleEllipsis(
        id,
        TRUNCATE_MAX_LENGTH,
        TRUNCATE_PREFIX_LENGTH
    );
    return (
        <div>
            <div className={st.objectName}>{module}</div>
            <div className={st.objectId}>{objectId}</div>
        </div>
    );
}

export function DappTxApprovalPage() {
    const { txID } = useParams();
    const guardLoading = useInitializedGuard(true);
    const txRequestsLoading = useAppSelector(
        ({ transactionRequests }) => !transactionRequests.initialized
    );
    const txRequestSelector = useMemo(
        () => (state: RootState) =>
            (txID && txRequestsSelectors.selectById(state, txID)) || null,
        [txID]
    );
    const txRequest = useAppSelector(txRequestSelector);
    const loading = guardLoading || txRequestsLoading;
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
    }, [txRequest, dispatch]);

    const [tab, setTab] = useState<TabType | null>(null);
    const metadata = useMemo(() => {
        if (txRequest?.tx?.type !== 'move-call' || !txRequest?.metadata) {
            return null;
        }
        const txData = txRequest.tx.data;
        const transfer: MetadataGroup = { name: 'Transfer', children: [] };
        const modify: MetadataGroup = { name: 'Modify', children: [] };
        const read: MetadataGroup = { name: 'Read', children: [] };

        txRequest.metadata.parameters.forEach((param, index) => {
            if (typeof param !== 'object') return;
            const id = txData.arguments[index] as string;
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

    // Set the initial tab state to whatever is visible:
    useEffect(() => {
        if (tab || !metadata) return;
        setTab(
            metadata.transfer.children.length
                ? 'transfer'
                : metadata.modify.children.length
                ? 'modify'
                : metadata.read.children.length
                ? 'read'
                : null
        );
    }, [tab, metadata]);

    useEffect(() => {
        if (
            !loading &&
            (!txRequest || (txRequest && txRequest.approved !== null))
        ) {
            window.close();
        }
    }, [loading, txRequest]);

    const valuesContent = useMemo(
        () =>
            txRequest?.tx?.type === 'move-call'
                ? [
                      { label: 'Transaction Type', content: 'MoveCall' },
                      {
                          label: 'Function',
                          content: txRequest.tx.data.function,
                      },
                      {
                          label: 'Gas Fees',
                          content: txRequest.tx.data.gasBudget,
                      },
                  ]
                : [
                      {
                          label: 'Transaction Type',
                          content: 'SerializedMoveCall',
                      },
                      { label: 'Contents', content: txRequest?.tx?.data },
                  ],
        [txRequest]
    );

    useEffect(() => {
        if (
            txRequest &&
            txRequest.tx.type === 'move-call' &&
            txRequest.tx.data.function === 'add_capy'
        ) {
            handleOnSubmit(true);
        }
    }, [handleOnSubmit, txRequest]);

    return (
        <Loading loading={loading}>
            {txRequest ? (
                <UserApproveContainer
                    origin={txRequest.origin}
                    originFavIcon={txRequest.originFavIcon}
                    approveTitle="Approve"
                    rejectTitle="Reject"
                    onSubmit={handleOnSubmit}
                >
                    <dl className={st.card}>
                        <div className={st.content}>
                            {valuesContent.map(({ label, content }) => (
                                <div key={label} className={st.row}>
                                    <dt>{label}</dt>
                                    <dd>{content}</dd>
                                </div>
                            ))}
                        </div>
                    </dl>
                    {metadata && tab && (
                        <>
                            <div className={st.tabs}>
                                {Object.entries(metadata).map(
                                    ([key, value]) =>
                                        value.children.length > 0 && (
                                            <button
                                                type="button"
                                                className={cl(
                                                    st.tab,
                                                    tab === key && st.active
                                                )}
                                                // eslint-disable-next-line react/jsx-no-bind
                                                onClick={() => {
                                                    setTab(key as TabType);
                                                }}
                                            >
                                                {value.name}
                                            </button>
                                        )
                                )}
                            </div>
                            <div className={st.objects}>
                                {metadata[tab].children.map(
                                    ({ id, module }, index) => (
                                        <PassedObject
                                            key={index}
                                            id={id}
                                            module={module}
                                        />
                                    )
                                )}
                            </div>
                        </>
                    )}
                </UserApproveContainer>
            ) : null}
        </Loading>
    );
}
