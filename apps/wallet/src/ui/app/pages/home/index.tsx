// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { Outlet } from 'react-router-dom';
import { of, filter, switchMap, from, defer, repeat } from 'rxjs';

import PageMainLayout from '_app/shared/page-main-layout';
import { useLockedGuard } from '_app/wallet/hooks';
import Loading from '_components/loading';
import { useInitializedGuard, useAppDispatch } from '_hooks';
import PageLayout from '_pages/layout';
import { fetchAllOwnedAndRequiredObjects } from '_redux/slices/sui-objects';
import { usePageView } from '_shared/utils';

const POLL_SUI_OBJECTS_INTERVAL = 4000;

interface Props {
    disableNavigation?: boolean;
    limitToPopUpSize?: boolean;
}

const HomePage = ({ disableNavigation, limitToPopUpSize = true }: Props) => {
    const initChecking = useInitializedGuard(true);
    const lockedChecking = useLockedGuard(false);
    const guardChecking = initChecking || lockedChecking;
    const dispatch = useAppDispatch();
    useEffect(() => {
        const sub = of(guardChecking)
            .pipe(
                filter(() => !guardChecking),
                switchMap(() =>
                    defer(() =>
                        from(dispatch(fetchAllOwnedAndRequiredObjects()))
                    ).pipe(repeat({ delay: POLL_SUI_OBJECTS_INTERVAL }))
                )
            )
            .subscribe();
        return () => sub.unsubscribe();
    }, [guardChecking, dispatch]);

    usePageView();
    return (
        <PageLayout limitToPopUpSize={limitToPopUpSize}>
            <Loading loading={guardChecking}>
                <PageMainLayout
                    bottomNavEnabled={!disableNavigation}
                    dappStatusEnabled={!disableNavigation}
                    topNavMenuEnabled={!disableNavigation}
                    centerLogo={true}
                >
                    <Outlet />
                </PageMainLayout>
            </Loading>
        </PageLayout>
    );
};

export default HomePage;
export { default as NftsPage } from './nfts';
export { default as TokensPage } from './tokens';
export { default as TransactionDetailsPage } from './transaction-details';
export { default as TransactionsPage } from './transactions';
export { default as TransferCoinPage } from './transfer-coin';
export { default as NFTDetailsPage } from './nft-details';
export { default as ReceiptPage } from './receipt';
export { default as CoinsSelectorPage } from './transfer-coin/CoinSelector';
export { default as AppsPage } from './apps';
