// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

import CoinBalance from '_components/coin-balance';
import ObjectsLayout from '_components/objects-layout';
import { useAppSelector } from '_hooks';
import { accountAggregateBalancesSelector } from '_redux/slices/account';

function TokensPage() {
    const balances = useAppSelector(accountAggregateBalancesSelector);
    const coinTypes = useMemo(() => Object.keys(balances), [balances]);
    return (
        <ObjectsLayout
            totalItems={coinTypes?.length}
            emptyMsg="No tokens found"
        >
            {coinTypes.map((aCoinType) => {
                const aCoinBalance = balances[aCoinType];
                return (
                    <CoinBalance
                        type={aCoinType}
                        balance={aCoinBalance}
                        stake={true}
                        key={aCoinType}
                    />
                );
            })}
        </ObjectsLayout>
    );
}

export default TokensPage;
