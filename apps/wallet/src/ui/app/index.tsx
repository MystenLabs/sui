// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB64 } from '@mysten/sui.js/utils';
import { useEffect, useMemo } from 'react';
import { Navigate, Outlet, Route, Routes, useLocation } from 'react-router-dom';

import { useSuiLedgerClient } from './components/ledger/SuiLedgerClientProvider';
import { useAccounts } from './hooks/useAccounts';
import { useBackgroundClient } from './hooks/useBackgroundClient';
import { useInitialPageView } from './hooks/useInitialPageView';

import { useStorageMigrationStatus } from './hooks/useStorageMigrationStatus';
import { AccountsDev } from './pages/AccountsDevPage';
import { StorageMigrationPage } from './pages/StorageMigrationPage';

import { AccountsPage } from './pages/accounts/AccountsPage';
import { AddAccountPage } from './pages/accounts/AddAccountPage';
import { BackupMnemonicPage } from './pages/accounts/BackupMnemonicPage';
import { ForgotPasswordPage } from './pages/accounts/ForgotPasswordPage';
import { ImportLedgerAccountsPage } from './pages/accounts/ImportLedgerAccountsPage';
import { ImportPassphrasePage } from './pages/accounts/ImportPassphrasePage';
import { ImportPrivateKeyPage } from './pages/accounts/ImportPrivateKeyPage';
import { ProtectAccountPage } from './pages/accounts/ProtectAccountPage';
import { WelcomePage } from './pages/accounts/WelcomePage';
import { ManageAccountsPage } from './pages/accounts/manage/ManageAccountsPage';

import { ApprovalRequestPage } from './pages/approval-request';
import HomePage, {
	AppsPage,
	AssetsPage,
	CoinsSelectorPage,
	KioskDetailsPage,
	NFTDetailsPage,
	NftTransferPage,
	OnrampPage,
	ReceiptPage,
	TransactionBlocksPage,
	TransferCoinPage,
} from './pages/home';
import TokenDetailsPage from './pages/home/tokens/TokenDetailsPage';
import { QredoConnectInfoPage } from './pages/qredo-connect/QredoConnectInfoPage';
import { SelectQredoAccountsPage } from './pages/qredo-connect/SelectQredoAccountsPage';

import { RestrictedPage } from './pages/restricted';
import SiteConnectPage from './pages/site-connect';
import { AppType } from './redux/slices/app/AppType';
import PageMainLayout from './shared/page-main-layout';
import { Staking } from './staking/home';

import { useAppDispatch, useAppSelector } from '_hooks';

import { setNavVisibility } from '_redux/slices/app';
import { isLedgerAccountSerializedUI } from '_src/background/accounts/LedgerAccount';
import { persistableStorage } from '_src/shared/analytics/amplitude';
import { type LedgerAccountsPublicKeys } from '_src/shared/messaging/messages/payloads/MethodPayload';

const HIDDEN_MENU_PATHS = [
	'/nft-details',
	'/nft-transfer',
	'/receipt',
	'/send',
	'/send/select',
	'/apps/disconnectapp',
];

const App = () => {
	const dispatch = useAppDispatch();
	const isPopup = useAppSelector((state) => state.app.appType === AppType.popup);
	useEffect(() => {
		document.body.classList.remove('app-initializing');
	}, [isPopup]);
	const location = useLocation();
	useEffect(() => {
		const menuVisible = !HIDDEN_MENU_PATHS.some((aPath) => location.pathname.startsWith(aPath));
		dispatch(setNavVisibility(menuVisible));
	}, [location, dispatch]);

	useInitialPageView();
	const { data: accounts } = useAccounts();
	const allLedgerWithoutPublicKey = useMemo(
		() => accounts?.filter(isLedgerAccountSerializedUI).filter(({ publicKey }) => !publicKey) || [],
		[accounts],
	);
	const backgroundClient = useBackgroundClient();
	const { connectToLedger, suiLedgerClient } = useSuiLedgerClient();
	useEffect(() => {
		if (accounts?.length) {
			// The user has accepted our terms of service after their primary
			// account has been initialized (either by creating a new wallet
			// or importing a previous account). This means we've gained
			// consent and can persist device data to cookie storage
			persistableStorage.persist();
		}
	}, [accounts]);
	useEffect(() => {
		// update ledger accounts without the public key
		(async () => {
			if (allLedgerWithoutPublicKey.length) {
				try {
					if (!suiLedgerClient) {
						await connectToLedger();
						return;
					}
					const publicKeysToStore: LedgerAccountsPublicKeys = [];
					for (const { derivationPath, id } of allLedgerWithoutPublicKey) {
						if (derivationPath) {
							try {
								const { publicKey } = await suiLedgerClient.getPublicKey(derivationPath);
								publicKeysToStore.push({
									accountID: id,
									publicKey: toB64(publicKey),
								});
							} catch (e) {
								// do nothing
							}
						}
					}
					if (publicKeysToStore.length) {
						await backgroundClient.storeLedgerAccountsPublicKeys({ publicKeysToStore });
					}
				} catch (e) {
					// do nothing
				}
			}
		})();
	}, [allLedgerWithoutPublicKey, suiLedgerClient, backgroundClient, connectToLedger]);

	const storageMigration = useStorageMigrationStatus();
	if (storageMigration.isLoading || !storageMigration?.data) {
		return null;
	}
	if (storageMigration.data !== 'ready') {
		return <StorageMigrationPage />;
	}
	return (
		<Routes>
			<Route path="forgot-password" element={<ForgotPasswordPage />} />
			<Route path="restricted" element={<RestrictedPage />} />
			<Route path="/*" element={<HomePage />}>
				<Route path="apps/*" element={<AppsPage />} />
				<Route path="kiosk" element={<KioskDetailsPage />} />
				<Route path="nft-details" element={<NFTDetailsPage />} />
				<Route path="nft-transfer/:nftId" element={<NftTransferPage />} />
				<Route path="nfts/*" element={<AssetsPage />} />
				<Route path="onramp" element={<OnrampPage />} />
				<Route path="receipt" element={<ReceiptPage />} />
				<Route path="send" element={<TransferCoinPage />} />
				<Route path="send/select" element={<CoinsSelectorPage />} />
				<Route path="stake/*" element={<Staking />} />
				<Route path="tokens/*" element={<TokenDetailsPage />} />
				<Route path="transactions/:status?" element={<TransactionBlocksPage />} />
				<Route path="*" element={<Navigate to="/tokens" replace={true} />} />
			</Route>
			<Route path="accounts/*" element={<AccountsPage />}>
				<Route path="welcome" element={<WelcomePage />} />
				<Route path="add-account" element={<AddAccountPage />} />
				<Route path="import-ledger-accounts" element={<ImportLedgerAccountsPage />} />
				<Route path="import-passphrase" element={<ImportPassphrasePage />} />
				<Route path="import-private-key" element={<ImportPrivateKeyPage />} />
				<Route path="manage" element={<ManageAccountsPage />} />
				<Route path="protect-account" element={<ProtectAccountPage />} />
				<Route path="backup/:accountSourceID" element={<BackupMnemonicPage />} />
				<Route
					path="qredo-connect/*"
					element={
						<PageMainLayout>
							<Outlet />
						</PageMainLayout>
					}
				>
					<Route path=":requestID" element={<QredoConnectInfoPage />} />
					<Route path=":id/select" element={<SelectQredoAccountsPage />} />
				</Route>
			</Route>
			<Route path="/account">
				<Route path="forgot-password" element={<ForgotPasswordPage />} />
			</Route>
			<Route path="/dapp/*" element={<HomePage disableNavigation />}>
				<Route path="connect/:requestID" element={<SiteConnectPage />} />
				<Route path="approve/:requestID" element={<ApprovalRequestPage />} />
			</Route>
			{process.env.NODE_ENV === 'development' ? (
				<Route path="/accounts-dev" element={<AccountsDev />} />
			) : null}
		</Routes>
	);
};

export default App;
