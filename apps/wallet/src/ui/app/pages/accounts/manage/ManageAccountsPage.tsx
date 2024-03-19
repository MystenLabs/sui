// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type AccountType } from '_src/background/accounts/Account';
import { useInitializedGuard } from '_src/ui/app/hooks';
import { useAccountGroups } from '_src/ui/app/hooks/useAccountGroups';
import { useNavigate } from 'react-router-dom';

import Overlay from '../../../components/overlay';
import { AccountGroup } from './AccountGroup';

export function ManageAccountsPage() {
	const navigate = useNavigate();
	const groupedAccounts = useAccountGroups();
	useInitializedGuard(true);
	return (
		<Overlay showModal title="Manage Accounts" closeOverlay={() => navigate('/home')}>
			<div className="flex flex-col gap-10 flex-1">
				{Object.entries(groupedAccounts).map(([type, accountGroups]) =>
					Object.entries(accountGroups).map(([key, accounts]) => {
						return (
							<AccountGroup
								key={`${type}-${key}`}
								accounts={accounts}
								accountSourceID={key}
								type={type as AccountType}
							/>
						);
					}),
				)}
			</div>
		</Overlay>
	);
}
