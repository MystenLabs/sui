// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getExecutionStatusType,
    getTotalGasUsed,
    getExecutionStatusError,
} from '@mysten/sui.js';
import * as Sentry from '@sentry/react';
import { useEffect, useState, useContext } from 'react';
import { useLocation, useParams } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';
import { NetworkContext } from '../../context';
import theme from '../../styles/theme.module.css';
import {
    DefaultRpcClient as rpc,
    type Network,
} from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import { findDataFromID } from '../../utils/static/searchUtil';
import { type DataType } from './TransactionResultType';
import TransactionView from './TransactionView';

import type {
    CertifiedTransaction,
    SuiTransactionResponse,
    ExecutionStatusType,
    TransactionEffects,
    SuiObjectRef,
    SuiEvent,
} from '@mysten/sui.js';

type TxnState = CertifiedTransaction & {
    loadState: string;
    txId: string;
    status: ExecutionStatusType;
    gasFee: number;
    txError: string;
    mutated: SuiObjectRef[];
    created: SuiObjectRef[];
    events?: SuiEvent[];
    timestamp_ms: number | null;
};
// TODO: update state to include Call types
// TODO: clean up duplicate fields
const initState: TxnState = {
    loadState: 'pending',
    txId: '',
    data: {
        transactions: [],
        sender: '',
        gasPayment: { digest: '', objectId: '', version: 0 },
        gasBudget: 0,
    },
    transactionDigest: '',
    txSignature: '',
    authSignInfo: {
        epoch: 0,
        signature: [],
    },
    status: 'success',
    gasFee: 0,
    txError: '',
    timestamp_ms: 0,
    mutated: [],
    created: [],
    events: [],
};

function fetchTransactionData(
    txId: string | undefined,
    network: Network | string
): Promise<SuiTransactionResponse> {
    try {
        if (!txId) {
            throw new Error('No Txid found');
        }
        return rpc(network)
            .getTransactionWithEffects(txId)
            .then((txEff: SuiTransactionResponse) => txEff);
    } catch (error) {
        throw error;
    }
}

const getCreatedOrMutatedData = (
    txEffects: TransactionEffects,
    contentType: 'created' | 'mutated'
): SuiObjectRef[] => {
    return contentType in txEffects && txEffects[contentType] != null
        ? txEffects[contentType]!.map((item) => item.reference)
        : [];
};

const FailedToGetTxResults = ({ id }: { id: string }) => (
    <ErrorResult
        id={id}
        errorMsg={
            !id
                ? "Can't search for a transaction without a digest"
                : 'Data could not be extracted for the following specified transaction ID'
        }
    />
);

const transformTransactionResponse = (
    txObj: SuiTransactionResponse,
    id: string
): TxnState => {
    return {
        ...txObj.certificate,
        status: getExecutionStatusType(txObj),
        gasFee: getTotalGasUsed(txObj),
        txError: getExecutionStatusError(txObj) ?? '',
        txId: id,
        loadState: 'loaded',
        mutated: getCreatedOrMutatedData(txObj.effects, 'mutated'),
        created: getCreatedOrMutatedData(txObj.effects, 'created'),
        events: txObj.effects().events,
        timestamp_ms: txObj.timestamp_ms,
    };
};

const TransactionResultAPI = ({ id }: { id: string }) => {
    const [showTxState, setTxState] = useState(initState);
    const [network] = useContext(NetworkContext);
    useEffect(() => {
        if (id == null) {
            return;
        }
        fetchTransactionData(id, network)
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
    }, [id, network]);

    // TODO update Loading screen
    if (showTxState.loadState === 'pending') {
        return (
            <div className={theme.textresults}>
                <div>Loading...</div>
            </div>
        );
    }
    if (id && showTxState.loadState === 'loaded') {
        return <TransactionResultLoaded txData={showTxState} />;
    }
    // For Batch transactions show error
    // TODO update Error screen and account for Batch transactions

    return <FailedToGetTxResults id={id} />;
};

const TransactionResultStatic = ({ id }: { id: string }) => {
    const entry = findDataFromID(id, undefined);
    try {
        return (
            <TransactionResultLoaded
                txData={transformTransactionResponse(entry, id)}
            />
        );
    } catch (error) {
        console.error(error);
        Sentry.captureException(error);
        return <FailedToGetTxResults id={id} />;
    }
};

const TransactionResultLoaded = ({ txData }: { txData: DataType }) => {
    return <TransactionView txdata={txData} />;
};

function TransactionResult() {
    const { id } = useParams();
    const { state } = useLocation();

    const checkStateHasData = (
        state: any
    ): state is { data: SuiTransactionResponse } => {
        return state !== null && 'data' in state;
    };

    const checkIsString = (value: any): value is string =>
        typeof value === 'string';

    if (checkStateHasData(state) && id) {
        return (
            <TransactionResultLoaded
                txData={transformTransactionResponse(state.data, id)}
            />
        );
    }

    if (checkIsString(id)) {
        return IS_STATIC_ENV ? (
            <TransactionResultStatic id={id} />
        ) : (
            <TransactionResultAPI id={id} />
        );
    }

    return <ErrorResult id={id} errorMsg={'ID not a valid string'} />;
}

export default TransactionResult;
