// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { AccountMultiSelect } from '../../components/accounts/AccountMultiSelect';
import { AccountsList } from '../../components/accounts/AccountsList';
import { useActiveAddress } from '../../hooks';
import { useAccounts } from '../../hooks/useAccounts';
import TokenDetails from '../home/tokens/TokensDetails';

export function TokensV2() {
	const accounts = useAccounts();
	const address = useActiveAddress();
	const [selectedAccounts, setSelectedAccounts] = useState<string[]>([address!]);

	return (
		<div className="flex flex-col gap-4">
			<TokenDetails />
		</div>
	);
}
