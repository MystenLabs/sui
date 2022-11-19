// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';
import cl from 'classnames';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useParams } from 'react-router-dom';

import { MiniNFT } from './MiniNFT';
import { SummeryCard } from './SummeryCard';
import AccountAddress from '_components/account-address';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import ExternalLink from '_components/external-link';
import Loading from '_components/loading';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import UserApproveContainer from '_components/user-approve-container';
import {
    useAppDispatch,
    useAppSelector,
    useMiddleEllipsis,
    useFormatCoin,
    useGetNFTMetaData,
} from '_hooks';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';
import {
    loadTransactionResponseMetadata,
    respondToTransactionRequest,
    txRequestsSelectors,
    deserializeTxn,
} from '_redux/slices/transaction-requests';
import { thunkExtras } from '_redux/store/thunk-extras';

import type {
    SuiMoveNormalizedType,
    MoveCallTransaction,
} from '@mysten/sui.js';
import type { RootState } from '_redux/RootReducer';

import st from './DappTxApprovalPage.module.scss';

interface MetadataGroup {
    name: string;
    children: { id: string; module: string }[];
    nftImage?: string;
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

function PassedObject({
    id,
    module,
    nftLink,
}: {
    id: string;
    module: string;
    nftLink?: string;
}) {
    const objectId = useMiddleEllipsis(
        id,
        TRUNCATE_MAX_LENGTH,
        TRUNCATE_PREFIX_LENGTH
    );

    return (
        <div className={st.permissionsContent}>
            <div className={st.permissionsContentLabel}>
                <ExplorerLink
                    type={ExplorerLinkType.object}
                    objectID={id}
                    className={st.objectId}
                    showIcon={false}
                >
                    {objectId}
                </ExplorerLink>
                <div className={st.objectName}>{module}</div>
            </div>

            {nftLink && nftLink && <MiniNFT url={nftLink} size="small" />}
        </div>
    );
}

type PermissionsProps = {
    metadata: {
        transfer: MetadataGroup;
        modify: MetadataGroup;
        read: MetadataGroup;
    } | null;
};

function Permissions({ metadata }: PermissionsProps) {
    const [tab, setTab] = useState<TabType | null>(null);
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
    return (
        metadata &&
        tab && (
            <SummeryCard header="Permissions requested">
                <div className={st.content}>
                    <div className={st.tabs}>
                        {Object.entries(metadata).map(
                            ([key, value]) =>
                                value.children.length > 0 && (
                                    <button
                                        type="button"
                                        key={key}
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
                        {metadata[tab].children.map(({ id, module }, index) => (
                            <PassedObject
                                key={index}
                                id={id}
                                nftLink={metadata[tab].nftImage}
                                module={module}
                            />
                        ))}
                    </div>
                </div>
            </SummeryCard>
        )
    );
}

type TransferSummaryProps = {
    label: string;
    content: string | number | null;
    loading: boolean;
};

const GAS_ESTIMATE_LABEL = 'Estimated Gas Fees';

function TransactionSummery({ label, content, loading }: TransferSummaryProps) {
    const isGasEstimate = label === GAS_ESTIMATE_LABEL;
    const [gasEstimate, symbol] = useFormatCoin(
        (isGasEstimate && content) || 0,
        GAS_TYPE_ARG
    );

    const valueContent =
        content === null
            ? '-'
            : isGasEstimate
            ? `${gasEstimate} ${symbol}`
            : content;
    return (
        <>
            <div className={st.label}>{label}</div>
            <div className={st.value}>
                {loading ? <LoadingIndicator /> : valueContent}
            </div>
        </>
    );
}

type TransferSummerCardProps = {
    coinSymbol: string | null;
    amount: number | null;
    origin: string;
    objectId: string | null;
    nftImage?: string | null;
    gasEstimate: number | null;
};

function MiniNFTLink({
    id,
    url,
    name,
}: {
    id: string;
    url: string;
    name?: string | null;
}) {
    const objectId = useMiddleEllipsis(
        id,
        TRUNCATE_MAX_LENGTH,
        TRUNCATE_PREFIX_LENGTH
    );

    return (
        <>
            <MiniNFT url={url} name={name} />
            <ExplorerLink
                type={ExplorerLinkType.object}
                objectID={id}
                className={st.objectId}
                showIcon={false}
            >
                {objectId}
            </ExplorerLink>
        </>
    );
}

function TransactionSummeryCard({
    coinSymbol,
    amount,
    objectId,
    origin,
    nftImage,
    gasEstimate,
}: TransferSummerCardProps) {
    const [gasEst, gasSymbol] = useFormatCoin(gasEstimate || 0, GAS_TYPE_ARG);

    const [formatedAmount, symbol] = useFormatCoin(
        amount ? Math.abs(amount) : 0,
        coinSymbol
    );

    return (
        <SummeryCard header="Transaction summary">
            <div className={st.content}>
                {formatedAmount && symbol && (
                    <>
                        <div className={st.row}>
                            <div className={st.label}>Send</div>
                            <div className={st.value}>
                                {formatedAmount} {symbol}
                            </div>
                        </div>

                        <div className={st.row}>
                            <div className={st.label}>To</div>
                            <div className={st.value}>
                                <ExternalLink
                                    href={origin}
                                    className={st.origin}
                                >
                                    {new URL(origin || '').host}
                                </ExternalLink>
                            </div>
                        </div>
                    </>
                )}
            </div>
            {nftImage && objectId && (
                <div className={st.content}>
                    <div className={st.row}>
                        <div className={st.label}>Send</div>
                        <div className={st.value}>
                            <MiniNFTLink id={objectId} url={nftImage} />
                        </div>
                    </div>
                    <div className={st.row}>
                        <div className={st.label}>To</div>
                        <div className={st.value}>
                            <AccountAddress showLink={false} />
                        </div>
                    </div>
                </div>
            )}
            <div className={st.cardFooter}>
                <div>Estimated Gas Fees</div>
                {gasEst} {gasSymbol}
            </div>
        </SummeryCard>
    );
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
    const address = useAppSelector(({ account }) => account.address);
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

    const nftMeta = useGetNFTMetaData(txRequest?.txnMeta?.objectId || null);

    const metadata = useMemo(() => {
        if (
            (txRequest?.tx?.type !== 'move-call' &&
                txRequest?.tx?.type !== 'serialized-move-call' &&
                !txRequest?.unSerializedTxn) ||
            !txRequest?.metadata
        ) {
            return null;
        }
        const txData =
            (txRequest?.unSerializedTxn?.data as MoveCallTransaction) ??
            txRequest.tx.data;

        const transfer: MetadataGroup = {
            name: 'Transfer',
            children: [],
            nftImage: nftMeta?.url,
        };
        const modify: MetadataGroup = { name: 'Modify', children: [] };
        const read: MetadataGroup = { name: 'Read', children: [] };

        txRequest.metadata.parameters.forEach((param, index) => {
            if (typeof param !== 'object') return;
            const id = txData?.arguments[index] as string;
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
    }, [
        nftMeta?.url,
        txRequest?.metadata,
        txRequest?.tx.data,
        txRequest?.tx?.type,
        txRequest?.unSerializedTxn,
    ]);

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

    const txGasEstimationResult = useQuery({
        queryKey: ['tx-request', 'gas-estimate', txRequest?.id, address],
        queryFn: () => {
            if (txRequest) {
                const signer = thunkExtras.api.getSignerInstance(
                    thunkExtras.keypairVault.getKeyPair()
                );
                let txToEstimate: Parameters<
                    typeof signer.dryRunTransaction
                >['0'];
                const txType = txRequest.tx.type;
                if (txType === 'v2' || txType === 'serialized-move-call') {
                    txToEstimate = txRequest.tx.data;
                } else {
                    txToEstimate = {
                        kind: 'moveCall',
                        data: txRequest.tx.data,
                    };
                }
                return signer.getGasCostEstimation(txToEstimate);
            }
            return Promise.resolve(null);
        },
        enabled: !!(txRequest && address),
    });

    const transactionSummery = txRequest?.txnMeta;

    const gasEstimation = txGasEstimationResult.data ?? null;

    const valuesContent: {
        label: string;
        content: string | number | null;
        loading?: boolean;
    }[] = useMemo(() => {
        switch (txRequest?.tx.type) {
            case 'v2': {
                return [
                    {
                        label: 'Transaction Type',
                        content: txRequest.tx.data.kind,
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
                                      ).function ?? '',
                              },
                              {
                                  label: 'Module',
                                  content:
                                      (
                                          txRequest?.unSerializedTxn
                                              ?.data as MoveCallTransaction
                                      ).module ?? '',
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

    const TransactionTypeHeader = (
        <>
            <div className={st.txTypeHeaderTitle}>Transaction Type</div>
            <div className={st.txTypeHeaderStatus}>
                {txRequest?.unSerializedTxn?.kind ?? txRequest?.tx?.type}
            </div>
        </>
    );

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
                        {transactionSummery && (
                            <TransactionSummeryCard
                                objectId={transactionSummery?.objectId || null}
                                amount={transactionSummery?.amount || null}
                                coinSymbol={
                                    transactionSummery?.coinSymbol || null
                                }
                                nftImage={nftMeta?.url}
                                gasEstimate={gasEstimation}
                                origin={txRequest.origin}
                            />
                        )}
                        <Permissions metadata={metadata} />
                        <SummeryCard
                            header={TransactionTypeHeader}
                            transparentHeader
                        >
                            <div className={st.content}>
                                {valuesContent.map(
                                    ({ label, content, loading = false }) => (
                                        <div key={label} className={st.row}>
                                            <TransactionSummery
                                                label={label}
                                                content={content}
                                                loading={loading}
                                            />
                                        </div>
                                    )
                                )}
                            </div>
                        </SummeryCard>
                    </section>
                </UserApproveContainer>
            ) : null}
        </Loading>
    );
}
