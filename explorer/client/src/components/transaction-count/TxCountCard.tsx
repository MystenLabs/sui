// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState, useContext } from 'react';

import { NetworkContext } from '../../context';
import {
    DefaultRpcClient as rpc,
    type Network,
} from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import ErrorResult from '../error-result/ErrorResult';

import styles from './TxCountCard.module.css';

const initState = { count: 0, loadState: 'pending' };

async function getTransactionCount(network: Network | string): Promise<number> {
    return rpc(network).getTotalTransactionNumber();
}

function TxCountCard({ count }: { count: number | string }) {
    return (
        <div className={styles.txcount} id="txcount">
            Total Transactions
            <div>{count}</div>
        </div>
    );
}

function TxCountCardStatic() {
    return <TxCountCard count={3030} />;
}

function TxCountCardAPI() {
    const [isLoaded, setIsLoaded] = useState(false);
    const [results, setResults] = useState(initState);
    const [network] = useContext(NetworkContext);
    useEffect(() => {
        let isMounted = true;
        getTransactionCount(network)
            .then((resp: number) => {
                if (isMounted) {
                    setIsLoaded(true);
                }
                setResults({
                    loadState: 'loaded',
                    count: resp,
                });
            })
            .catch((err) => {
                setResults({
                    ...initState,
                    loadState: 'fail',
                });
                setIsLoaded(false);
            });

        return () => {
            isMounted = false;
        };
    }, [network]);
    if (results.loadState === 'pending') {
        return <TxCountCard count="" />;
    }

    if (!isLoaded && results.loadState === 'fail') {
        return (
            <ErrorResult
                id=""
                errorMsg="Error getting total transaction count"
            />
        );
    }

    return <TxCountCard count={results.count} />;
}

const LatestTxCard = () =>
    IS_STATIC_ENV ? <TxCountCardStatic /> : <TxCountCardAPI />;

export default LatestTxCard;
