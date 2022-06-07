// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useContext } from 'react';

import { NetworkContext } from '../../context';
import { DefaultRpcClient as rpc } from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import { findTxfromID } from '../../utils/static/searchUtil';
import ErrorResult from '../error-result/ErrorResult';
import Longtext from '../longtext/Longtext';

const DATATYPE_DEFAULT = {
    to: [],
    from: [],
    loadState: 'pending',
};

const getTx = async (id: string, network: string, category: 'address') =>
    rpc(network).getTransactionsForAddress(id);

function TxForIDView({
    showData,
}: {
    showData: { to: string[][] | never[]; from: string[][] | never[] };
}) {
    const deduplicate = (results: string[][]) =>
        results
            .map((result) => result[1])
            .filter((value, index, self) => self.indexOf(value) === index);

    return (
        <>
            <div>
                <div>Transactions Sent</div>
                <div id="txFrom">
                    {deduplicate(showData.from).map((x, index) => (
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
            <div>
                <div>Transactions Received</div>
                <div id="txTo">
                    {deduplicate(showData.to).map((x, index) => (
                        <div key={`to-${index}`}>
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

function TxForIDStatic({ id, category }: { id: string; category: 'address' }) {
    const showData = findTxfromID(id);
    if (showData?.to?.[0] && showData?.from?.[0]) {
        return <TxForIDView showData={showData} />;
    } else {
        return <></>;
    }
}

function TxForIDAPI({ id, category }: { id: string; category: 'address' }) {
    const [showData, setData] = useState(DATATYPE_DEFAULT);
    const [network] = useContext(NetworkContext);
    useEffect(() => {
        getTx(id, network, category)
            .then((data) =>
                setData({
                    ...(data as typeof DATATYPE_DEFAULT),
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
        return <TxForIDView showData={showData} />;
    }

    return (
        <ErrorResult
            id={id}
            errorMsg="Transactions could not be extracted on the following specified transaction ID"
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
