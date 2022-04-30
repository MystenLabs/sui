// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';
import theme from '../../styles/theme.module.css';
import { DefaultRpcClient as rpc } from '../../utils/api/DefaultRpcClient';

import styles from './LastestTxCard.module.css';

const initState = {
    loadState: 'pending',
    lastestTx: [0, ''],
};

const getRecentTransactions = async (txNum:number) => { 
    try {
        // Get the lastest transactions
        const transactions = await rpc.getRecentTransactions(txNum).then((res: any) => res);
        console.log(transactions);
        return await Promise.all(transactions.map(async (tx: any) => {
             return {
                 block: tx[0],
                 txId: tx[1],
                 txData: await rpc.getTransaction(tx[1]).then((res: any) => res).catch((err: any) => false),
             }
            }));  
    } catch (error) {
        throw error
    }
}
;
function LastestTxCard() {
    const [isLoaded, setIsLoaded] = useState(false);
    const [results, setResults] = useState(initState);
    useEffect(() => {
        console.log(getRecentTransactions(5));
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
                <div className={styles.transactioncard}>
                    <div className={styles.txcard}>
                        <div className={styles.txcardgrid}>
                            <h3>Latest Transaction</h3>
                        </div>
                        {results.lastestTx.map((tx: any, index: number) => (
                            <div key={index} className={styles.txcardgrid}>
                                <div className={styles.txcardgridlarge}>
                                    <div>{tx[0]}</div>
                                    <Link
                                        className={styles.txlink}
                                        to={
                                            'transactions/' +
                                            encodeURIComponent(tx[1])
                                        }
                                    >
                                        {tx[1]}
                                    </Link>
                                </div>
                            </div>
                        ))}
                    </div>
                </div>
            </div>
        </>
    );
}

export default LastestTxCard;
