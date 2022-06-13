// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { memo, useEffect } from 'react';

import TransactionResult from '_components/transactions-card';
import { useAppSelector, useAppDispatch } from '_hooks';
import { getTransactionsByAddress } from '_redux/slices/txresults';

function TransactionPage() {
    const dispatch = useAppDispatch();
    const txByAddress = useAppSelector(({ txresults }) => txresults.latestTx);

    useEffect(() => {
        dispatch(getTransactionsByAddress()).unwrap();
    }, [dispatch]);

    return txByAddress && txByAddress.length ? (
        <TransactionResult
            txresults={txByAddress.filter((_, index: number) => index <= 4)}
        />
    ) : null;
}

export default memo(TransactionPage);
