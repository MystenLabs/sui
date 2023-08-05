// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Navigate, Route, Routes } from 'react-router-dom';
import { AccountsPage } from './pages/accounts/AccountsPage';
import { AddAccountPage } from './pages/accounts/AddAccountPage';
import { ImportLedgerAccountsPage } from './pages/accounts/ImportLedgerAccountsPage';
import { ImportPassphrasePage } from './pages/accounts/ImportPassphrasePage';
import { ImportPrivateKeyPage } from './pages/accounts/ImportPrivateKeyPage';
import { ManageAccountsPage } from './pages/accounts/Manage';
import { ProtectAccountPage } from './pages/accounts/ProtectAccountPage';
import { TokensV2 } from './pages/enoki-onboarding/TokensV2';
import HomePage from './pages/home';
import TokenDetailsPage from './pages/home/tokens/TokenDetailsPage';
import TokenDetails from './pages/home/tokens/TokensDetails';
import LockedPage from './wallet/locked-page';
import { ForgotPasswordPage as ForgotPasswordPageV2 } from '_pages/accounts/ForgotPasswordPage';
import { WelcomePage as WelcomePageV2 } from '_pages/enoki-onboarding/WelcomePage';

export function EnokiRoutes() {
	return (
		<Routes>
			<Route path="/*" element={<HomePage />}>
				<Route path="home" element={<TokenDetails />} />
				<Route path="*" element={<Navigate to="/home" />} />
			</Route>
			<Route path="/account">
				<Route path="forgot-password" element={<ForgotPasswordPageV2 />} />
			</Route>
			<Route path="/accounts" element={<AccountsPage />}>
				<Route path="manage" element={<ManageAccountsPage />} />
				<Route path="add-account" element={<AddAccountPage />} />
				<Route path="protect-account" element={<ProtectAccountPage />} />
				<Route path="import-ledger-accounts" element={<ImportLedgerAccountsPage />} />
				<Route path="import-passphrase" element={<ImportPassphrasePage />} />
				<Route path="import-private-key" element={<ImportPrivateKeyPage />} />
			</Route>
			<Route path="/welcome" element={<WelcomePageV2 />} />
			<Route path="locked" element={<LockedPage />} />
		</Routes>
	);
}
