// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    isGetTxnDigestsResponse,
    type GetTxnDigestsResponse,
} from '@mysten/sui.js';
import { useState, useEffect, useContext } from 'react';

import { NetworkContext } from '../../context';
import { DefaultRpcClient as rpc } from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import { findTxfromID } from '../../utils/static/searchUtil';
import ErrorResult from '../error-result/ErrorResult';
import Longtext from '../longtext/Longtext';

const DATATYPE_DEFAULT = {
    loadState: 'pending',
};

const getTx = async (
    id: string,
    network: string,
    category: 'address'
): Promise<GetTxnDigestsResponse> => rpc(network).getTransactionsForAddress(id);

function TxForIDView({
    showData,
}: {
    showData: [number, string][] | undefined;
}) {
    if (!showData) return <></>;
    const deduplicate = (results: [number, string][]) =>
        results
            .map((result) => result[1])
            .filter((value, index, self) => self.indexOf(value) === index);

    return (
        <>
            <div>
                <div>Transactions</div>
                <div id="tx">
                    {deduplicate(showData).map((x, index) => (
                        <div key={`from-${index}`}>
                            <Longtext
                                text={x}
                                category="transactions"
                                isLink={true}
                            />
                        </div>
                    ))}
                </div>
            </div>
        </>
    );
}

function HandleMethodUndefined({ data }: { data: any }) {
    if (isGetTxnDigestsResponse(data)) {
        return <TxForIDView showData={data} />;
    } else {
        return <></>;
    }
}

function TxForIDStatic({ id, category }: { id: string; category: 'address' }) {
    const data = findTxfromID(id)?.data;
    return <HandleMethodUndefined data={data} />;
}

function TxForIDAPI({ id, category }: { id: string; category: 'address' }) {
    const [showData, setData] =
        useState<{ data?: GetTxnDigestsResponse; loadState: string }>(
            DATATYPE_DEFAULT
        );
    const [network] = useContext(NetworkContext);
    useEffect(() => {
        getTx(id, network, category)
            .then((data) =>
                setData({
                    data: data,
                    loadState: 'loaded',
                })
            )
            .catch((error) => {
                console.log(error);
                setData({ ...DATATYPE_DEFAULT, loadState: 'fail' });
            });
    }, [id, network, category]);

    if (showData.loadState === 'pending') {
        return <div>Loading ...</div>;
    }

    if (showData.loadState === 'loaded') {
        const data = showData.data;
        return <HandleMethodUndefined data={data} />;
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
    category: 'address';
}) {
    return IS_STATIC_ENV ? (
        <TxForIDStatic id={id} category={category} />
    ) : (
        <TxForIDAPI id={id} category={category} />
    );
}
