// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { AccountMultiSelect } from '../../components/accounts/AccountMultiSelect';
import { AccountsList } from '../../components/accounts/AccountsList';
import { useActiveAddress } from '../../hooks';
import { useAccounts } from '../../hooks/useAccounts';

export function TokensV2() {
	const accounts = useAccounts();
	const address = useActiveAddress();
	const [selectedAccounts, setSelectedAccounts] = useState<string[]>([address!]);

	return (
		<div className="flex flex-col gap-4">
			<AccountsList />
			<div className="bg-gray-40 -mx-5 p-5 h-full">
				<AccountMultiSelect
					accounts={accounts}
					selectedAccounts={selectedAccounts}
					onChange={setSelectedAccounts}
				/>
			</div>
		</div>
	);
}
