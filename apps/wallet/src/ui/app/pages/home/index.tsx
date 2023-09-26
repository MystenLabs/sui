// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Loading from '_components/loading';
import { useInitializedGuard } from '_hooks';
import { useSetGrowthbookAttributes } from '_shared/utils';
import { PageMainLayout } from '_src/ui/app/shared/page-main-layout/PageMainLayout';
import { Outlet } from 'react-router-dom';

interface Props {
	disableNavigation?: boolean;
}

const HomePage = ({ disableNavigation }: Props) => {
	const initChecking = useInitializedGuard(true);
	const guardChecking = initChecking;

	useSetGrowthbookAttributes();
	return (
		<Loading loading={guardChecking}>
			<PageMainLayout
				bottomNavEnabled={!disableNavigation}
				dappStatusEnabled={!disableNavigation}
				topNavMenuEnabled={!disableNavigation}
			>
				<Outlet />
			</PageMainLayout>
		</Loading>
	);
};

export default HomePage;
export { default as NftsPage } from './nfts';
export { default as HiddenAssetsPage } from './hidden-assets';
export { default as AssetsPage } from './assets';
export { default as TokensPage } from './tokens';
export { default as TransactionBlocksPage } from './transactions';
export { default as TransferCoinPage } from './transfer-coin';
export { default as NFTDetailsPage } from './nft-details';
export { default as NftTransferPage } from './nft-transfer';
export { default as KioskDetailsPage } from './kiosk-details';
export { default as ReceiptPage } from './receipt';
export { default as CoinsSelectorPage } from './transfer-coin/CoinSelector';
export { default as AppsPage } from './apps';
export { Onramp as OnrampPage } from './onramp';
