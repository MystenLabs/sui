// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';
import { useParams } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';
// import Longtext from '../../components/longtext/Longtext';
import TransactionCard from '../../components/transaction-card/TransactionCard';
import theme from '../../styles/theme.module.css';
import { findDataFromID } from '../../utils/static/searchUtil';

import styles from './TransactionResult.module.css';

// Todo update state to include Call types
const initState = {
    loadState: 'pending',
    txId: false,
    transaction: {
        data: {
            kind: {
                Single: {
                    Transfer: {
                        recipient: '',
                        object_ref: ['', 0, ''],
                    },
                },
            },
            sender: '',
            gas_payment: ['', 0, ''],
            gas_budget: 0,
        },
        tx_signature: '',
        auth_signature: '',
    },
    signatures: [],
};

const isStatic = process.env.REACT_APP_DATA !== 'static';

function TransactionResult() {
    const { id } = useParams();
    const [showTxState, setTxState] = useState(initState);

    // if dev fetch data from mock_data.json
    // add delay to simulate barckend service
    // Remove this section in production
    const fetchTransactionData = async (txId: string | undefined) => {
        try {
            if (!txId) {
                throw new Error('No Txid found');
            }

            // Use Mockdata in dev
            if (!isStatic) {
                // resolve after one second
                return new Promise((resolve, reject) => {
                    setTimeout(() => {
                        const staticObj = findDataFromID(txId, undefined);
                        if (!staticObj) {
                            reject('txid not found');
                        }
                        resolve(staticObj);
                    }, 1000);
                });
            }

            // TODO add BackendService API
            // Test response data using Sui.js
            // throw error for now
            throw new Error('Error');
        } catch (error) {
            throw error;
        }
    };

    useEffect(() => {
        fetchTransactionData(id)
            .then((resp: any) => {
                setTxState({
                    ...resp,
                    txId: id,
                    loadState: 'loaded',
                });
                return;
            })
            .catch((err) => {
                setTxState({
                    ...initState,
                    loadState: 'fail',
                });
                return;
            });
    }, [id]);

    // TODO update Loading screen
    if (showTxState.loadState === 'pending') {
        return (
            <div className={theme.textresults}>
                <div className={styles.content}>Loading...</div>
            </div>
        );
    }

    // TODO update Error screen
    if (!id || showTxState.loadState === 'fail') {
        return (
            <ErrorResult
                id={id}
                errorMsg={
                    !id
                        ? "Can't search for a transaction without a digest"
                        : 'There was an issue with the data on the following transaction'
                }
            />
        );
    }

    return (
        <div className={theme.textresults}>
            <div><h3>Transaction Details</h3> </div>
            {showTxState.loadState === 'loaded' && (
                <div className={styles.content}>
                    <TransactionCard txdata={showTxState} />
                </div>
            )}
        </div>
    );
}

export default TransactionResult;
// export { instanceOfDataType };
