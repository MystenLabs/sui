// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useNavigate } from 'react-router-dom';
import { AccountGroup } from './AccountGroup';

import Overlay from '../../../components/overlay';
import { useAccounts } from '../../../hooks/useAccounts';

export function ManageAccountsPage() {
	const { data: accounts = [] } = useAccounts();
	const navigate = useNavigate();

	return (
		<Overlay showModal title="Manage Accounts" closeOverlay={() => navigate('/home')}>
			<div className="flex flex-col gap-4 flex-1">
				<AccountGroup accounts={accounts} type="mnemonic-derived" />
			</div>
		</Overlay>
	);
}
