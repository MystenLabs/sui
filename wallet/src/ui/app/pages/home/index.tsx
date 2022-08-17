// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useEffect } from 'react';
import { Link, Outlet } from 'react-router-dom';
import { of, filter, switchMap, from, defer, repeat } from 'rxjs';

import Loading from '_components/loading';
import Logo from '_components/logo';
import { MenuButton, MenuContent } from '_components/menu';
import Navigation from '_components/navigation';
import { useInitializedGuard, useAppDispatch } from '_hooks';
import PageLayout from '_pages/layout';
import { fetchAllOwnedAndRequiredObjects } from '_redux/slices/sui-objects';

import st from './Home.module.scss';

const POLL_SUI_OBJECTS_INTERVAL = 4000;

interface Props {
    disableNavigation?: boolean;
    limitToPopUpSize?: boolean;
}

const HomePage = ({ disableNavigation, limitToPopUpSize = true }: Props) => {
    const guardChecking = useInitializedGuard(true);
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

    return (
        <PageLayout limitToPopUpSize={limitToPopUpSize}>
            <Loading loading={guardChecking}>
                <div className={st.container}>
                    <div className={st.header}>
                        <span />
                        <Link to="/tokens" className={st.logoLink}>
                            <Logo className={st.logo} txt={true} />
                        </Link>
                        {disableNavigation ? (
                            <span />
                        ) : (
                            <MenuButton className={st.menuButton} />
                        )}
                    </div>
                    <div className={st.content}>
                        <main
                            className={cl(
                                st.main,
                                !disableNavigation && st.withNav
                            )}
                        >
                            <Outlet />
                        </main>
                        {!disableNavigation && <Navigation />}
                        <MenuContent />
                    </div>
                </div>
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
