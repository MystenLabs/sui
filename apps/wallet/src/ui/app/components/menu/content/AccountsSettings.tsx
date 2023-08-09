// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LockLocked16 as LockedLockIcon } from '@mysten/icons';
import { Outlet, useNavigate } from 'react-router-dom';
import Browser from 'webextension-polyfill';

import { Account } from './Account';
import { MenuLayout } from './MenuLayout';
import { useNextMenuUrl } from '_components/menu/hooks';
import { AppType } from '_redux/slices/app/AppType';
import { ampli } from '_src/shared/analytics/ampli';
import { useAppSelector } from '_src/ui/app/hooks';
import { useAccounts } from '_src/ui/app/hooks/useAccounts';
import { useDeriveNextAccountMutation } from '_src/ui/app/hooks/useDeriveNextAccountMutation';
import { Button } from '_src/ui/app/shared/ButtonUI';

export function AccountsSettings() {
	const backUrl = useNextMenuUrl(true, '/');
	const importPrivateKeyUrl = useNextMenuUrl(true, '/import-private-key');
	const accounts = useAccounts();
	const createAccountMutation = useDeriveNextAccountMutation();
	const navigate = useNavigate();
	const appType = useAppSelector((state) => state.app.appType);
	const connectLedgerModalUrl = useNextMenuUrl(true, '/accounts/connect-ledger-modal');

	return (
		<MenuLayout title="Accounts" back={backUrl}>
			<div className="flex flex-col gap-3">
				{accounts.map((account) => (
					<Account key={account.address} account={account} />
				))}
				<Button
					variant="outline"
					size="tall"
					text="Create New Account"
					loading={createAccountMutation.isLoading}
					onClick={() => createAccountMutation.mutate()}
				/>
				<Button variant="outline" size="tall" text="Import Private Key" to={importPrivateKeyUrl} />
				<Button
					variant="outline"
					size="tall"
					text="Connect Ledger Wallet"
					before={<LockedLockIcon />}
					onClick={async () => {
						ampli.openedConnectLedgerFlow({ sourceFlow: 'Account menu' });
						if (appType === AppType.popup) {
							const { origin, pathname } = window.location;
							await Browser.tabs.create({
								url: `${origin}${pathname}#${connectLedgerModalUrl}`,
							});
							window.close();
						} else {
							navigate(connectLedgerModalUrl);
						}
					}}
				/>
				<Outlet />
			</div>
		</MenuLayout>
	);
}
