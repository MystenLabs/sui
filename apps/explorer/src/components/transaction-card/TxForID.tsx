// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type GetTxnDigestsResponse } from '@mysten/sui.js';
import { useState, useEffect, useContext } from 'react';

import { NetworkContext } from '../../context';
import { DefaultRpcClient as rpc } from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import { deduplicate } from '../../utils/searchUtil';
import { findTxfromID, findTxDatafromID } from '../../utils/static/searchUtil';
import ErrorResult from '../error-result/ErrorResult';
import PaginationLogic from '../pagination/PaginationLogic';
import {
    type TxnData,
    genTableDataFromTxData,
    getDataOnTxDigests,
} from './TxCardUtils';

import TableCard from '~/ui/TableCard';

const TRUNCATE_LENGTH = 14;
const ITEMS_PER_PAGE = 20;

const DATATYPE_DEFAULT = {
    loadState: 'pending',
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
    if (!showData || showData.length === 0) return null;

    const tableData = genTableDataFromTxData(showData, TRUNCATE_LENGTH);

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
    const data = deduplicate(findTxfromID(id)?.data as string[] | undefined)
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
                        setData({
                            data: data as TxnData[],
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
