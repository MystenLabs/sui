// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Filter16, Plus12 } from '@mysten/icons';
import * as ToggleGroup from '@radix-ui/react-toggle-group';
import { useMemo } from 'react';
import { AccountListItem } from './AccountListItem';
import { FooterLink } from './FooterLink';
import { useAccountGroups } from '../../hooks/useAccountGroups';
import { useActiveAccount } from '../../hooks/useActiveAccount';
import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { Heading } from '../../shared/heading';

import { ampli } from '_src/shared/analytics/ampli';
import { Collapsible } from '_src/ui/app/shared/collapse';

export function AccountsList() {
	const accountGroups = useAccountGroups();
	const accounts = accountGroups.list();
	const activeAccount = useActiveAccount();
	const backgroundClient = useBackgroundClient();

	const otherAccounts = useMemo(
		() => accounts.filter((a) => a.id !== activeAccount?.id) || [],
		[accounts, activeAccount?.id],
	);

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
	if (!accounts || !activeAccount) return null;

	return (
		<div className="bg-gradients-graph-cards flex flex-col rounded-xl p-4 gap-5 border border-solid border-hero/10 w-full select-none">
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
								<AccountListItem account={activeAccount} editable />
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
												<AccountListItem account={account} />
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
		</div>
	);
}
