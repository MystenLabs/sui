// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getSingleTransactionKind,
    getExecutionStatusType,
    getTotalGasUsed,
    getExecutionDetails,
} from '@mysten/sui.js';
import cl from 'classnames';
import { useEffect, useState } from 'react';
import { useParams } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';
import theme from '../../styles/theme.module.css';
import { DefaultRpcClient as rpc } from '../../utils/api/DefaultRpcClient';
import { findDataFromID } from '../../utils/static/searchUtil';
import { type DataType } from './TransactionResultType';
import TransactionView from './TransactionView';

import type {
    CertifiedTransaction,
    TransactionEffectsResponse,
    ExecutionStatusType,
    TransactionEffects,
    RawObjectRef,
} from '@mysten/sui.js';

import styles from './TransactionResult.module.css';

type TxnState = CertifiedTransaction & {
    loadState: string;
    txId: string;
    status: ExecutionStatusType;
    gasFee: number;
    txError: string;
    mutated: RawObjectRef[];
    created: RawObjectRef[];
};
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
    status: 'Success',
    gasFee: 0,
    txError: '',
    mutated: [],
    created: [],
};

const useRealData = process.env.REACT_APP_DATA !== 'static';

function fetchTransactionData(
    txId: string | undefined
): Promise<TransactionEffectsResponse> {
    try {
        if (!txId) {
            throw new Error('No Txid found');
        }
        return rpc
            .getTransactionWithEffects(txId)
            .then((txEff: TransactionEffectsResponse) => txEff);
    } catch (error) {
        throw error;
    }
}

const getCreatedOrMutatedData = (
    txEffects: TransactionEffects,
    contentType: 'created' | 'mutated'
) => {
    // Get the first item in the 'created' | 'mutated' array
    return contentType in txEffects
        ? txEffects[contentType].map((itm) => itm[0])
        : [];
};

const transformTransactionResponse = (
    txObj: TransactionEffectsResponse,
    id: string
) => {
    const executionStatus = txObj.effects.status;
    const status = getExecutionStatusType(executionStatus);
    const details = getExecutionDetails(executionStatus);
    return {
        ...txObj.certificate,
        status,
        gasFee: getTotalGasUsed(executionStatus),
        txError:
            'error' in details
                ? details.error[Object.keys(details.error)[0]].error
                : '',
        txId: id,
        loadState: 'loaded',
        mutated: getCreatedOrMutatedData(txObj.effects, 'mutated'),
        created: getCreatedOrMutatedData(txObj.effects, 'created'),
    };
};

const TransactionResultAPI = ({ id }: { id: string }) => {
    const [showTxState, setTxState] = useState(initState);
    useEffect(() => {
        if (id == null) {
            return;
        }
        fetchTransactionData(id)
            .then((txObj) => {
                setTxState(transformTransactionResponse(txObj, id));
            })
            .catch((err) => {
                console.log('Error fetching transaction data', err);
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
                <div>Loading...</div>
            </div>
        );
    }
    if (
        id &&
        showTxState.loadState === 'loaded' &&
        getSingleTransactionKind(showTxState.data) !== null
    ) {
        return <TransactionResultLoaded txData={showTxState} />;
    }
    // For Batch transactions show error
    // TODO update Error screen and account for Batch transactions

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
};

const TransactionResultStatic = ({ id }: { id: string }) => {
    const entry = findDataFromID(id, undefined);

    return (
        <TransactionResultLoaded
            txData={transformTransactionResponse(entry, id)}
        />
    );
};

const TransactionResultLoaded = ({ txData }: { txData: DataType }) => {
    return (
        <div className={cl(theme.textresults, styles.txdetailsbg)}>
            <div className={theme.txdetailstitle}>
                <h3>Transaction Details</h3>
            </div>
            <TransactionView txdata={txData} />
        </div>
    );
};

function TransactionResult() {
    const { id } = useParams();

    if (typeof id === 'string') {
        return useRealData ? (
            <TransactionResultAPI id={id as string} />
        ) : (
            <TransactionResultStatic id={id as string} />
        );
    }

    return <ErrorResult id={id} errorMsg={'ID not a valid string'} />;
}

export default TransactionResult;
