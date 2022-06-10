// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type GetTxnDigestsResponse,
    type ExecutionStatusType,
    type TransactionKindName,
} from '@mysten/sui.js';
import cl from 'classnames';
import { useState, useEffect, useContext } from 'react';
import { Link } from 'react-router-dom';

import { NetworkContext } from '../../context';
import {
    DefaultRpcClient as rpc,
    getDataOnTxDigests,
} from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import { deduplicate } from '../../utils/searchUtil';
import { findTxfromID, findTxDatafromID } from '../../utils/static/searchUtil';
import { truncate } from '../../utils/stringUtils';
import ErrorResult from '../error-result/ErrorResult';
import Longtext from '../longtext/Longtext';

import styles from './TxForID.module.css';

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
};

const getTx = async (
    id: string,
    network: string,
    category: 'address'
): Promise<GetTxnDigestsResponse> => rpc(network).getTransactionsForAddress(id);

function TxForIDView({ showData }: { showData: TxnData[] | undefined }) {
    if (!showData || showData.length === 0) return <></>;

    return (
        <>
            <div>
                <div>Transactions</div>
                <div id="tx">
                    <div className={styles.txheader}>
                        <div className={styles.txid}>TxId</div>
                        <div className={styles.txtype}>TxType</div>
                        <div className={styles.txstatus}>Status</div>
                        <div className={styles.txadd}>Addresses</div>
                    </div>

                    {showData.map((x, index) => (
                        <div key={`txid-${index}`} className={styles.txrow}>
                            <div className={styles.txid}>
                                <Longtext
                                    text={x.txId}
                                    category="transactions"
                                    isLink={true}
                                />
                            </div>
                            <div className={styles.txtype}>{x.kind}</div>
                            <div
                                className={cl(
                                    styles.txstatus,
                                    styles[x.status.toLowerCase()]
                                )}
                            >
                                {x.status === 'success' ? '\u2714' : '\u2716'}
                            </div>
                            <div className={styles.txadd}>
                                <div>
                                    From:
                                    <Link
                                        className={styles.txlink}
                                        to={'addresses/' + x.From}
                                    >
                                        {truncate(x.From, 14, '...')}
                                    </Link>
                                </div>
                                {x.To && (
                                    <div>
                                        To :
                                        <Link
                                            className={styles.txlink}
                                            to={'addresses/' + x.To}
                                        >
                                            {truncate(x.To, 14, '...')}
                                        </Link>
                                    </div>
                                )}
                            </div>
                        </div>
                    ))}
                </div>
            </div>
        </>
    );
}

function TxForIDStatic({ id, category }: { id: string; category: 'address' }) {
    const data = deduplicate(
        findTxfromID(id)?.data as [number, string][] | undefined
    )
        .map((id) => findTxDatafromID(id))
        .filter((x) => x !== undefined) as TxnData[];
    if (!data) return <></>;
    return <TxForIDView showData={data} />;
}

function TxForIDAPI({ id, category }: { id: string; category: 'address' }) {
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
        return <TxForIDView showData={data} />;
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
