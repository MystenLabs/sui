// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useLocation, useParams } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';
import theme from '../../styles/theme.module.css';
import TransactionView from './TransactionView';

import type { SuiTransactionResponse } from '@mysten/sui.js';

import { useGetTransaction } from '~/hooks/useGetTransaction';

function FailedToGetTxResults({ id }: { id: string }) {
    return (
        <ErrorResult
            id={id}
            errorMsg={
                !id
                    ? "Can't search for a transaction without a digest"
                    : 'Data could not be extracted for the following specified transaction ID'
            }
        />
    );
}

function TransactionResultAPI({ id }: { id: string }) {
    const { isLoading, isError, data } = useGetTransaction(id);

    // TODO update Loading screen
    if (isLoading) {
        return (
            <div className={theme.textresults}>
                <div>Loading...</div>
            </div>
        );
    }

    if (isError || !data) {
        return <FailedToGetTxResults id={id} />;
    }

    return <TransactionView transaction={data} />;
}

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
        return <TransactionView transaction={state.data} />;
    }

    if (checkIsString(id)) {
        return <TransactionResultAPI id={id} />;
    }

    return <ErrorResult id={id} errorMsg="ID not a valid string" />;
}

export default TransactionResult;
