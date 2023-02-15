// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useLocation, useParams } from 'react-router-dom';

import TransactionView from './TransactionView';

import type { SuiTransactionResponse } from '@mysten/sui.js';

import { useGetTransaction } from '~/hooks/useGetTransaction';
import { Banner } from '~/ui/Banner';
import { LoadingSpinner } from '~/ui/LoadingSpinner';

function FailedToGetTxResults({ id }: { id: string }) {
    return (
        <Banner variant="error" spacing="lg" fullWidth>
            {!id
                ? "Can't search for a transaction without a digest"
                : `Data could not be extracted for the following specified transaction ID: ${id}`}
        </Banner>
    );
}

function TransactionResultAPI({ id }: { id: string }) {
    const { isLoading, isError, data } = useGetTransaction(id);

    // TODO update Loading screen
    if (isLoading) {
        return <LoadingSpinner text="Loading..." />;
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
    ): state is { data: SuiTransactionResponse } =>
        state !== null && 'data' in state;

    const checkIsString = (value: any): value is string =>
        typeof value === 'string';

    if (checkStateHasData(state) && id) {
        return <TransactionView transaction={state.data} />;
    }

    if (checkIsString(id)) {
        return <TransactionResultAPI id={id} />;
    }

    return (
        <Banner variant="error" spacing="lg" fullWidth>
            ID &ldquo;
            {id}
            &rdquo; is not a valid string
        </Banner>
    );
}

export default TransactionResult;
