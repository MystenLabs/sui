// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Filter16, Plus12 } from '@mysten/icons';
import * as ToggleGroup from '@radix-ui/react-toggle-group';
import { useMemo, useState } from 'react';
import { AccountListItem } from './AccountListItem';
import { FooterLink } from './FooterLink';
import { UnlockAccountModal } from './UnlockAccountModal';
import { useActiveAccount } from '../../hooks/useActiveAccount';
import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { Heading } from '../../shared/heading';

import { ampli } from '_src/shared/analytics/ampli';
import { useAccounts } from '_src/ui/app/hooks/useAccounts';
import { Collapsible } from '_src/ui/app/shared/collapse';

export function AccountsList() {
	const { data: accounts } = useAccounts();
	const activeAccount = useActiveAccount();
	const backgroundClient = useBackgroundClient();

	// todo: these will be grouped by account type
	const otherAccounts = useMemo(
		() => accounts?.filter((a) => a.id !== activeAccount?.id) || [],
		[accounts, activeAccount?.id],
	);

	// todo: replace this with a real flow
	const [unlockModalOpen, setUnlockModalOpen] = useState(false);
	const handleUnlockAccount = () => {
		setUnlockModalOpen(true);
	};

	const close = () => setUnlockModalOpen(false);

	const handleSelectAccount = async (accountID: string) => {
		const account = accounts?.find((a) => a.id === accountID);
		if (!account) return;
		if (accountID !== activeAccount?.id) {
			ampli.switchedAccount({
				toAccountType: account.type,
			});
			await backgroundClient.selectAccount(accountID);
		}
	};
	if (!accounts || !activeAccount) {
		return null;
	}
	return (
		<div className="bg-gradients-graph-cards flex flex-col rounded-xl p-4 gap-5 border border-solid border-hero/10 w-full">
			<Heading variant="heading5" weight="semibold" color="steel-darker">
				Accounts
			</Heading>

			<ToggleGroup.Root
				asChild
				value={activeAccount.id}
				type="single"
				onValueChange={handleSelectAccount}
			>
				<>
					<Collapsible title="Current" defaultOpen shade="darker">
						<ToggleGroup.Item asChild value={activeAccount.id}>
							<div>
								<AccountListItem
									address={activeAccount.address}
									handleUnlockAccount={handleUnlockAccount}
								/>
							</div>
						</ToggleGroup.Item>
					</Collapsible>

					{otherAccounts.length ? (
						<Collapsible title="Switch To" shade="darker">
							<div className="flex flex-col gap-3">
								{otherAccounts.map((account) => {
									return (
										<ToggleGroup.Item asChild key={account.id} value={account.id}>
											<div>
												<AccountListItem
													address={account.address}
													handleUnlockAccount={handleUnlockAccount}
												/>
											</div>
										</ToggleGroup.Item>
									);
								})}
							</div>
						</Collapsible>
					) : null}
				</>
			</ToggleGroup.Root>

			<div className="flex justify-between">
				<FooterLink color="steelDarker" icon={<Filter16 />} to="/accounts/manage" text="Manage" />
				<FooterLink color="steelDarker" icon={<Plus12 />} to="/accounts/add-account" text="Add" />
			</div>
			{unlockModalOpen ? <UnlockAccountModal onClose={close} onSuccess={close} /> : null}
		</div>
	);
}
