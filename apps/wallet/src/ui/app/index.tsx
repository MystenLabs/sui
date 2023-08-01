// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeatureIsOn } from '@growthbook/growthbook-react';
import { useEffect } from 'react';
import { Navigate, Route, Routes, useLocation } from 'react-router-dom';

import { useInitialPageView } from './hooks/useInitialPageView';
import { ProtectAccountPage } from './pages/accounts/ProtectAccountPage';
import { TokensV2 } from './pages/enoki-onboarding/TokensV2';
import AssetsPage from './pages/home/assets';
import { QredoConnectInfoPage } from './pages/qredo-connect/QredoConnectInfoPage';
import { SelectQredoAccountsPage } from './pages/qredo-connect/SelectQredoAccountsPage';
import { RestrictedPage } from './pages/restricted';
import WelcomePage from './pages/welcome';
import { AppType } from './redux/slices/app/AppType';
import { Staking } from './staking/home';
import ForgotPasswordPage from '_app/wallet/forgot-password-page';
import LockedPage from '_app/wallet/locked-page';
import { useAppDispatch, useAppSelector } from '_hooks';
import { AccountsPage } from '_pages/accounts/AccountsPage';
import { AddAccountPage } from '_pages/accounts/AddAccountPage';
import { ForgotPasswordPage as ForgotPasswordPageV2 } from '_pages/accounts/ForgotPasswordPage';
import { ImportLedgerAccountsPage } from '_pages/accounts/ImportLedgerAccountsPage';
import { ImportPassphrasePage } from '_pages/accounts/ImportPassphrasePage';
import { ImportPrivateKeyPage } from '_pages/accounts/ImportPrivateKeyPage';
import { ApprovalRequestPage } from '_pages/approval-request';
import { WelcomePage as WelcomePageV2 } from '_pages/enoki-onboarding/WelcomePage';
import HomePage, {
	TokensPage,
	TransactionBlocksPage,
	TransferCoinPage,
	NFTDetailsPage,
	ReceiptPage,
	KioskDetailsPage,
	CoinsSelectorPage,
	AppsPage,
	NftTransferPage,
	OnrampPage,
} from '_pages/home';
import InitializePage from '_pages/initialize';
import BackupPage from '_pages/initialize/backup';
import CreatePage from '_pages/initialize/create';
import { ImportPage } from '_pages/initialize/import';
import SelectPage from '_pages/initialize/select';
import SiteConnectPage from '_pages/site-connect';
import { setNavVisibility } from '_redux/slices/app';

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
	const isSocialSignInEnabled = useFeatureIsOn('enoki-social-sign-in');
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

	return (
		<Routes>
			<Route path="/*" element={<HomePage />}>
				<Route path="tokens/*" element={<TokensPage />} />
				<Route path="nfts/*" element={<AssetsPage />} />
				<Route path="apps/*" element={<AppsPage />} />
				<Route path="nft-details" element={<NFTDetailsPage />} />
				<Route path="kiosk" element={<KioskDetailsPage />} />
				<Route path="nft-transfer/:nftId" element={<NftTransferPage />} />
				<Route path="transactions/:status?" element={<TransactionBlocksPage />} />
				<Route path="send" element={<TransferCoinPage />} />
				<Route path="send/select" element={<CoinsSelectorPage />} />
				<Route path="stake/*" element={<Staking />} />
				<Route path="receipt" element={<ReceiptPage />} />
				<Route path="onramp" element={<OnrampPage />} />
				<Route path="*" element={<Navigate to="/tokens" replace={true} />} />
			</Route>

			<Route path="/dapp/*" element={<HomePage disableNavigation />}>
				<Route path="connect/:requestID" element={<SiteConnectPage />} />
				<Route path="approve/:requestID" element={<ApprovalRequestPage />} />
				<Route path="qredo-connect/:requestID" element={<QredoConnectInfoPage />} />
				<Route path="qredo-connect/:id/select" element={<SelectQredoAccountsPage />} />
			</Route>

			<Route path="welcome" element={isSocialSignInEnabled ? <WelcomePageV2 /> : <WelcomePage />} />
			{isSocialSignInEnabled && (
				<>
					<Route path="/*" element={<HomePage />}>
						<Route path="home" element={<TokensV2 />} />
					</Route>
					<Route path="/account">
						<Route path="forgot-password" element={<ForgotPasswordPageV2 />} />
					</Route>
					<Route path="/accounts" element={<AccountsPage />}>
						<Route path="add-account" element={<AddAccountPage />} />
						<Route path="protect-account" element={<ProtectAccountPage />} />
						<Route path="import-ledger-accounts" element={<ImportLedgerAccountsPage />} />
						<Route path="import-passphrase" element={<ImportPassphrasePage />} />
						<Route path="import-private-key" element={<ImportPrivateKeyPage />} />
					</Route>
				</>
			)}
			<Route path="/initialize" element={<InitializePage />}>
				<Route path="select" element={<SelectPage />} />
				<Route path="create" element={<CreatePage />} />
				<Route path="import" element={<ImportPage />} />
				<Route path="backup" element={<BackupPage />} />
				<Route path="backup-imported" element={<BackupPage mode="imported" />} />
			</Route>
			<Route path="locked" element={<LockedPage />} />
			<Route path="forgot-password" element={<ForgotPasswordPage />} />
			<Route path="restricted" element={<RestrictedPage />} />
		</Routes>
	);
};

export default App;
