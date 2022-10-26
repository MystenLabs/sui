// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useSearchParams } from 'react-router-dom';

import PageTitle from '_app/shared/page-title';
import ActiveCoinsCard from '_components/active-coins-card';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

import st from './CoinSelector.module.scss';

function CoinsSelectorPage() {
    const [searchParams] = useSearchParams();
    const coinType = searchParams.get('type') || GAS_TYPE_ARG;

    return (
        <div className={st.container}>
            <PageTitle
                title="Select Coin"
                backLink={`/send?${new URLSearchParams({
                    type: coinType,
                }).toString()}`}
                className={st.pageTitle}
            />
            <div className={cl(st.searchCoin, 'sui-icons-search')}>
                <input
                    type="text"
                    name="name"
                    placeholder="Search coins"
                    className={st.searchInput}
                />
            </div>
            <ActiveCoinsCard activeCoinType={coinType} showActiveCoin={false} />
        </div>
    );
}

export default CoinsSelectorPage;
