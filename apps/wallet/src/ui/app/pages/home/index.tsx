// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';
import { Outlet } from 'react-router-dom';

import PageMainLayout from '_app/shared/page-main-layout';
import { useLockedGuard } from '_app/wallet/hooks';
import Loading from '_components/loading';
import { useInitializedGuard } from '_hooks';
import PageLayout from '_pages/layout';
import { usePageView } from '_shared/utils';

interface Props {
    disableNavigation?: boolean;
    limitToPopUpSize?: boolean;
}

const HomePage = ({ disableNavigation, limitToPopUpSize = true }: Props) => {
    const initChecking = useInitializedGuard(true);
    const lockedChecking = useLockedGuard(false);
    const guardChecking = initChecking || lockedChecking;
    const [, setVisibility] = useState(document?.visibilityState || null);

    useEffect(() => {
        const callback = () => {
            setVisibility(document.visibilityState);
        };
        callback();
        document.addEventListener('visibilitychange', callback);
        return () => {
            document.removeEventListener('visibilitychange', callback);
        };
    }, []);

    usePageView();
    return (
        <PageLayout limitToPopUpSize={limitToPopUpSize}>
            <Loading loading={guardChecking}>
                <PageMainLayout
                    bottomNavEnabled={!disableNavigation}
                    dappStatusEnabled={!disableNavigation}
                    topNavMenuEnabled={!disableNavigation}
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
export { default as TransactionsPage } from './transactions';
export { default as TransferCoinPage } from './transfer-coin';
export { default as NFTDetailsPage } from './nft-details';
export { default as NftTransferPage } from './nft-transfer';
export { default as ReceiptPage } from './receipt';
export { default as CoinsSelectorPage } from './transfer-coin/CoinSelector';
export { default as AppsPage } from './apps';
