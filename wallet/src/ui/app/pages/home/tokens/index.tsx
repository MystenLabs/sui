// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

import AccountAddress from '_components/account-address';
import Alert from '_components/alert';
import BsIcon from '_components/bs-icon';
import CoinBalance from '_components/coin-balance';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useAppSelector } from '_hooks';
import { accountBalancesSelector } from '_redux/slices/account';

import st from './TokensPage.module.scss';

function TokensPage() {
    const balances = useAppSelector(accountBalancesSelector);
    const objectsLoading = useAppSelector(
        ({ suiObjects }) => suiObjects.loading
    );
    const objectsLastSync = useAppSelector(
        ({ suiObjects }) => suiObjects.lastSync
    );
    const objectsError = useAppSelector(({ suiObjects }) => suiObjects.error);
    const showError =
        !!objectsError &&
        (!objectsLastSync || Date.now() - objectsLastSync > 30 * 1000);
    const coinTypes = useMemo(() => Object.keys(balances), [balances]);
    const showEmptyNotice = !!(objectsLastSync && !coinTypes.length);
    const showTokens = !!(objectsLastSync && coinTypes.length);
    return (
        <div className={st.container}>
            <div>
                <span className={st.title}>Active Account:</span>
                <AccountAddress />
            </div>
            <div className={st.items}>
                {showError ? (
                    <Alert className={st.alert}>
                        <strong>Sync error (data might be outdated).</strong>{' '}
                        <small>{objectsError.message}</small>
                    </Alert>
                ) : null}
                {showTokens
                    ? coinTypes.map((aCoinType) => {
                          const aCoinBalance = balances[aCoinType];
                          return (
                              <CoinBalance
                                  type={aCoinType}
                                  balance={aCoinBalance}
                                  key={aCoinType}
                              />
                          );
                      })
                    : null}
                {showEmptyNotice ? (
                    <div className={st.empty}>
                        <BsIcon icon="droplet" className={st['empty-icon']} />
                        <div className={st['empty-text']}>No tokens found</div>
                    </div>
                ) : null}
                {objectsLoading && !objectsLastSync ? (
                    <div className={st.loader}>
                        <LoadingIndicator />
                    </div>
                ) : null}
            </div>
        </div>
    );
}

export default TokensPage;
