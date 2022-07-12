// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type GetTxnDigestsResponse,
    type ExecutionStatusType,
    type TransactionKindName,
} from '@mysten/sui.js';
import cl from 'classnames';
import { useState, useEffect, useContext } from 'react';

import { NetworkContext } from '../../context';
import {
    DefaultRpcClient as rpc,
    getDataOnTxDigests,
} from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import { deduplicate } from '../../utils/searchUtil';
import { findTxfromID, findTxDatafromID } from '../../utils/static/searchUtil';
import { truncate } from '../../utils/stringUtils';
import { timeAgo } from '../../utils/timeUtils';
import ErrorResult from '../error-result/ErrorResult';
import Longtext from '../longtext/Longtext';
import PaginationWrapper from '../pagination/PaginationWrapper';

import styles from './TxForID.module.css';

const TRUNCATE_LENGTH = 14;

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
    if (!showData || showData.length === 0) return <></>;

    return (
        <div id="tx" className={styles.txresults}>
            <div className={styles.txheader}>
                <div className={styles.txid}>TxId</div>
                {showData[0].timestamp_ms && (
                    <div className={styles.txage}>Time</div>
                )}
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
                            alttext={truncate(x.txId, 26, '...')}
                        />
                    </div>
                    {x.timestamp_ms && (
                        <div className={styles.txage}>
                            {`${timeAgo(x.timestamp_ms)} ago`}
                        </div>
                    )}
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
                            <Longtext
                                text={x.From}
                                category="addresses"
                                isLink={true}
                                isCopyButton={false}
                                alttext={truncate(x.From, TRUNCATE_LENGTH)}
                            />
                        </div>
                        {x.To && (
                            <div>
                                To :
                                <Longtext
                                    text={x.To}
                                    category="addresses"
                                    isLink={true}
                                    isCopyButton={false}
                                    alttext={truncate(x.To, TRUNCATE_LENGTH)}
                                />
                            </div>
                        )}
                    </div>
                </div>
            ))}
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
    return <PaginationWrapper results={data} viewComponentFn={viewFn} />;
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
        return <PaginationWrapper results={data} viewComponentFn={viewFn} />;
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
