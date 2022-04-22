// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useParams } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';
import Longtext from '../../components/longtext/Longtext';
import theme from '../../styles/theme.module.css';
import { findDataFromID } from '../../utils/static/searchUtil';

import { type SingleTransactionKind, type Transfer, type CertifiedTransaction, isCertifiedTransaction, isMoveCall, isMoveModulePublish, isSingleTransactionKind, isTransfer } from 'sui.js';

import styles from './TransactionResult.module.css';
import { DefaultRpcClient as rpc } from '../../utils/api/DefaultRpcClient';
import { useEffect, useState } from 'react';
import { type Loadable } from '../../utils/loadState';


const initState: Loadable<CertifiedTransaction> = {
    loadState: 'pending',
    transaction: {
        data: {
            kind: {
                Single: {
                    Transfer: {
                        recipient: '',
                        object_ref: ['', 0, '']
                    }
                }
            },
            sender: '',
            gas_payment: ['', 0, ''],
            gas_budget: 0
        },
        tx_signature: '',
        auth_signature: ''
    },
    signatures: []
}

const isStatic = process.env.REACT_APP_DATA !== 'static';

function TransactionResult() {
    const { digest } = useParams();
    const [showTxState, setTxState] = useState(initState);

    useEffect(() => {
        if (!digest)
            return;

        if (isStatic) {
            const staticObj = findDataFromID(digest, undefined);
            if(staticObj) {
                setTxState({
                    ...staticObj,
                    loadState: 'loaded',
                });
            } else {
                setTxState({
                    ...initState,
                    loadState: 'fail',
                });
            }
            return;
        }

        // load transaction data from the backend
        rpc.getTransaction(digest)
            .then((objState) => {
                setTxState({
                    ...objState,
                    loadState: 'loaded',
                });
            })
            .catch((error) => {
                console.log(error);
                setTxState({
                    ...initState,
                    loadState: 'fail',
                });
            });
    }, [digest]);

    if(!digest) {
        return (
            <ErrorResult
                id={digest}
                errorMsg="Can't search for a transaction without a digest"
            />
        )
    }

    if (process.env.REACT_APP_DATA !== 'static') {

        return (
            <div className={theme.textresults}>
                <div>This page is in Development</div>
            </div>
        );
    }

    if (isCertifiedTransaction(showTxState)) {
        const data = showTxState;
        const tx = data.transaction;
        const txData = tx.data;

        let singleTx: SingleTransactionKind | null = null;
        let transferTx: Transfer | null = null;

        if ('Single' in txData.kind && isSingleTransactionKind(txData.kind.Single)) {
            singleTx = txData.kind.Single;
            if ('Transfer' in singleTx && isTransfer(singleTx)) {
                transferTx = singleTx;
                // const transfer = singleTx.Transfer;
                // decide how to display Transfer transactions here
            }
            if ('Call' in singleTx && isMoveCall(singleTx.Call)) {
                // const call = singleTx.Call;
                // decide how to handle Move Call transactions here
            }
            if ('Publish' in singleTx && isMoveModulePublish(singleTx.Publish)) {
                // const publish = singleTx.Publish;
                // decide how to handle Publish transactions here (last priority)
            }
        }
        else if('Batch' in txData.kind) {
            // decide how to handle batch transaction display here
        }
        else {
            // decide how to handle invalid response data here
        }

        // right now we only get access to certified transactions, which have all succeeded
        const status = 'success';
        const statusClass = 'success';

        let action: string = 'unknown';
        let objectIDs: string[];
        let actionClass;

        switch (action) {
            case 'Create':
                actionClass = styles['action-create'];
                break;
            case 'Delete':
                actionClass = styles['action-delete'];
                break;
            case 'Fail':
                actionClass = styles['status-fail'];
                break;
            default:
                actionClass = styles['action-mutate'];
        }


        return (
            <div className={theme.textresults} id="textResults">
                <div>
                    <div>Transaction ID</div>
                    <div id="digest">
                        <Longtext
                            text={digest}
                            category="transactions"
                            isLink={false}
                        />
                    </div>
                </div>

                <div>
                    <div>Status</div>
                    <div id="transactionStatus" className={statusClass}>
                        {status}
                    </div>
                </div>

                <div>
                    <div>From</div>
                    <div>
                        <Longtext text={txData.sender} category="addresses" />
                    </div>
                </div>

                <div>
                    <div>Event</div>
                    <div className={actionClass}>{action}</div>
                </div>

                {transferTx != null && (
                    <div>
                        <div>To</div>
                        <div key={`recipient-0`}>
                            <Longtext
                                text={transferTx.recipient}
                                category="addresses"
                            />
                        </div>
                    </div>
                )}
            </div>
        );
    }
    return (
        <ErrorResult
            id={digest}
            errorMsg="There was an issue with the data on the following transaction"
        />
    );
}

export default TransactionResult;
