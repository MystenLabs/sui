// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useMemo } from 'react';
import { Navigate, useSearchParams } from 'react-router-dom';

import TokenDetails from './TokensDetails';
import PageTitle from '_app/shared/page-title';
import { GAS_TYPE_ARG, Coin } from '_redux/slices/sui-objects/Coin';

import st from './TokensPage.module.scss';

function TokenDetailsPage() {
    const [searchParams] = useSearchParams();
    const coinType = searchParams.get('type');
    const symbol = useMemo(
        () => (coinType ? Coin.getCoinSymbol(coinType) : ''),
        [coinType]
    );

    if (!coinType) {
        return <Navigate to="/tokens" replace={true} />;
    }
    return (
        <div className={st.detailsPage}>
            <PageTitle title={symbol} backLink="/tokens" hideBackLabel={true} />
            <TokenDetails coinType={GAS_TYPE_ARG} />
        </div>
    );
}

export default TokenDetailsPage;
