// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useEffect, useState } from 'react';
import { useParams } from 'react-router-dom';
import { getSingleTransactionKind } from 'sui.js';

import ErrorResult from '../../components/error-result/ErrorResult';
import TransactionCard from '../../components/transaction-card/TransactionCard';
import theme from '../../styles/theme.module.css';
import { DefaultRpcClient as rpc } from '../../utils/api/DefaultRpcClient';
import { findDataFromID } from '../../utils/static/searchUtil';

import type { CertifiedTransaction } from 'sui.js';

import styles from './TransactionResult.module.css';

type TxnState = CertifiedTransaction & { loadState: string; txId: string };
// Todo update state to include Call types
const initState: TxnState = {
    loadState: 'pending',
    txId: '',
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
    auth_sign_info: {
        epoch: 0,
        signatures: [],
    },
};

const isStatic = process.env.REACT_APP_DATA !== 'static';
// if dev fetch data from mock_data.json
function fetchTransactionData(txId: string | undefined) {
    try {
        if (!txId) {
            throw new Error('No Txid found');
        }
        // add delay to simulate barckend service
        // Remove this section in production
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

        return rpc.getTransaction(txId).then((objState) => objState);
    } catch (error) {
        throw error;
    }
}

function TransactionResult() {
    const { id } = useParams();
    const [showTxState, setTxState] = useState(initState);

    useEffect(() => {
        fetchTransactionData(id)
            .then((resp: any) => {
                setTxState({
                    ...resp,
                    txId: id,
                    loadState: 'loaded',
                });
            })
            .catch((err) => {
                //  remove this section in production
                setTxState({
                    ...initState,
                    loadState: 'fail',
                });
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
    // For Batch transactions show error
    // TODO update Error screen and account for Batch transactions
    if (
        !id ||
        showTxState.loadState === 'fail' ||
        getSingleTransactionKind(showTxState.data) == null
    ) {
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
        <div className={cl(theme.textresults, styles.txdetailsbg)}>
            <div className={theme.txdetailstitle}>
                <h3>Transaction Details</h3>
            </div>
            {showTxState.loadState === 'loaded' && (
                <TransactionCard txdata={showTxState} />
            )}
        </div>
    );
}

export default TransactionResult;
// export { instanceOfDataType };
