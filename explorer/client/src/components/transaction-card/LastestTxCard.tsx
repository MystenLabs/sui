// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';

import ErrorResult from '../../components/error-result/ErrorResult';
import Longtext from '../../components/longtext/Longtext';
import theme from '../../styles/theme.module.css';
import { DefaultRpcClient as rpc } from '../../utils/api/DefaultRpcClient';

import styles from './LastestTxCard.module.css';

const initState = {
    loadState: 'pending',
    lastestTx: [0, ''],
};
function LastestTxCard() {
    const [isLoaded, setIsLoaded] = useState(false);
    const [results, setResults] = useState(initState);
    useEffect(() => {
        rpc.getRecentTransactions(20)
            .then((resp: any[]) => {
                setResults({
                    loadState: 'loaded',
                    lastestTx: resp,
                });
                setIsLoaded(true);
            })
            .catch((err) => {
                setResults({
                    ...initState,
                });
            });
    }, []);
    if (!isLoaded) {
        return (
            <div className={theme.textresults}>
                <div className={styles.content}>Loading...</div>
            </div>
        );
    }

    if (!isLoaded && results.loadState === 'fail') {
        return (
            <ErrorResult
                id="lastestTx"
                errorMsg="There was an issue getting the lastest transaction"
            />
        );
    }

    return (
        <>
            <div className={theme.textresults}>
                <div className={theme.txdetailstitle}>
                    <h3>Latest transaction</h3>{' '}
                </div>
                <div className={styles.transactioncard}>
                    <div className={styles.txcard}>
                        {results.lastestTx.map((tx: any, index: number) => (
                            <Longtext
                                text={encodeURIComponent(tx[1])}
                                key={index}
                                category="transactions"
                                isLink={true}
                            />
                        ))}
                    </div>
                </div>
            </div>
        </>
    );
}

export default LastestTxCard;
