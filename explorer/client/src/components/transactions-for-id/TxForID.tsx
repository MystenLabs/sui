// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type GetTxnDigestsResponse,
    type ExecutionStatusType,
    type TransactionKindName,
} from '@mysten/sui.js';
import { useState, useEffect, useContext, useMemo } from 'react';

import { NetworkContext } from '../../context';
import {
    DefaultRpcClient as rpc,
    getDataOnTxDigests,
} from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import { numberSuffix } from '../../utils/numberUtil';
import { deduplicate } from '../../utils/searchUtil';
import { findTxfromID, findTxDatafromID } from '../../utils/static/searchUtil';
import { truncate } from '../../utils/stringUtils';
import { timeAgo } from '../../utils/timeUtils';
import ErrorResult from '../error-result/ErrorResult';
import PaginationLogic from '../pagination/PaginationLogic';
import TableCard from '../table/TableCard';

const TRUNCATE_LENGTH = 14;
const ITEMS_PER_PAGE = 20;

const DATATYPE_DEFAULT = {
    loadState: 'pending',
};

type TxnData = {
    seq: number;
    txId: string;
    status: ExecutionStatusType;
    kind: TransactionKindName | undefined;
    From: string;
    To?: string;
    timestamp_ms?: number | null;
    txGas: number;
};

type categoryType = 'address' | 'object';

const getTx = async (
    id: string,
    network: string,
    category: categoryType
): Promise<GetTxnDigestsResponse> =>
    category === 'address'
        ? rpc(network).getTransactionsForAddress(id)
        : rpc(network).getTransactionsForObject(id);

const viewFn = (results: any) => <TxForIDView showData={results} />;

function TxForIDView({ showData }: { showData: TxnData[] | undefined }) {
    // TODO: Ideally move this to a prop:
    const hasTimeColumn = showData?.[0]?.timestamp_ms;

    const tableData = useMemo(
        () => ({
            data: (showData ?? []).map((data) => ({
                date: `${timeAgo(data.timestamp_ms, undefined, true)} ago`,
                txTypes: {
                    txTypeName: data.kind,
                    status: data.status,
                },
                transactionId: [
                    {
                        url: data.txId,
                        name: truncate(data.txId, 26, '...'),
                        category: 'transactions',
                        isLink: true,
                        copy: false,
                    },
                ],
                addresses: [
                    {
                        url: data.From,
                        name: truncate(data.From, TRUNCATE_LENGTH),
                        category: 'addresses',
                        isLink: true,
                        copy: false,
                    },
                    ...(data.To
                        ? [
                              {
                                  url: data.To,
                                  name: truncate(data.To, TRUNCATE_LENGTH),
                                  category: 'addresses',
                                  isLink: true,
                                  copy: false,
                              },
                          ]
                        : []),
                ],
                gas: numberSuffix(data.txGas),
            })),
            columns: [
                ...(hasTimeColumn
                    ? [
                          {
                              headerLabel: 'Time',
                              accessorKey: 'date',
                          },
                      ]
                    : []),
                {
                    headerLabel: 'TxType',
                    accessorKey: 'txTypes',
                },
                {
                    headerLabel: 'Transaction ID',
                    accessorKey: 'transactionId',
                },
                {
                    headerLabel: 'Addresses',
                    accessorKey: 'addresses',
                },
                {
                    headerLabel: 'Gas',
                    accessorKey: 'gas',
                },
            ],
        }),
        [hasTimeColumn, showData]
    );

    if (!showData || showData.length === 0) return null;

    return (
        <div data-testid="tx">
            <TableCard tabledata={tableData} />
        </div>
    );
}

function TxForIDStatic({
    id,
    category,
}: {
    id: string;
    category: categoryType;
}) {
    const data = deduplicate(
        findTxfromID(id)?.data as [number, string][] | undefined
    )
        .map((id) => findTxDatafromID(id))
        .filter((x) => x !== undefined) as TxnData[];
    if (!data) return <></>;
    return (
        <PaginationLogic
            results={data}
            viewComponentFn={viewFn}
            itemsPerPage={ITEMS_PER_PAGE}
            canVaryItemsPerPage
        />
    );
}

function TxForIDAPI({ id, category }: { id: string; category: categoryType }) {
    const [showData, setData] =
        useState<{ data?: TxnData[]; loadState: string }>(DATATYPE_DEFAULT);
    const [network] = useContext(NetworkContext);
    useEffect(() => {
        getTx(id, network, category).then((transactions) => {
            //If the API method does not exist, the transactions will be undefined
            if (!transactions?.[0]) {
                setData({
                    loadState: 'loaded',
                });
            } else {
                getDataOnTxDigests(network, transactions)
                    .then((data) => {
                        const subData = data.map((el) => ({
                            seq: el!.seq,
                            txId: el!.txId,
                            status: el!.status,
                            kind: el!.kind,
                            From: el!.From,
                            To: el!.To,
                            txGas: el!.txGas,
                            timestamp_ms: el!.timestamp_ms,
                        }));
                        setData({
                            data: subData,
                            loadState: 'loaded',
                        });
                    })
                    .catch((error) => {
                        console.log(error);
                        setData({ ...DATATYPE_DEFAULT, loadState: 'fail' });
                    });
            }
        });
    }, [id, network, category]);

    if (showData.loadState === 'pending') {
        return <div>Loading ...</div>;
    }

    if (showData.loadState === 'loaded') {
        const data = showData.data;
        if (!data) return <></>;
        return (
            <PaginationLogic
                results={data}
                viewComponentFn={viewFn}
                itemsPerPage={ITEMS_PER_PAGE}
                canVaryItemsPerPage
            />
        );
    }

    return (
        <ErrorResult
            id={id}
            errorMsg="Transactions could not be extracted on the following specified ID"
        />
    );
}

export default function TxForID({
    id,
    category,
}: {
    id: string;
    category: categoryType;
}) {
    return IS_STATIC_ENV ? (
        <TxForIDStatic id={id} category={category} />
    ) : (
        <TxForIDAPI id={id} category={category} />
    );
}
