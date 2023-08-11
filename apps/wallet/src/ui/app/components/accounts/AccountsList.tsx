// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Filter16, Plus12 } from '@mysten/icons';
import * as ToggleGroup from '@radix-ui/react-toggle-group';
import { useState } from 'react';
import { AccountListItem } from './AccountListItem';
import { FooterLink } from './FooterLink';
import { UnlockAccountModal } from './UnlockAccountModal';
import { useActiveAddress } from '../../hooks';
import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { Heading } from '../../shared/heading';

import { ampli } from '_src/shared/analytics/ampli';
import { useAccounts } from '_src/ui/app/hooks/useAccounts';
import { Collapsible } from '_src/ui/app/shared/collapse';

export function AccountsList() {
	const activeAddress = useActiveAddress();
	const accounts = useAccounts();
	const backgroundClient = useBackgroundClient();

	// todo: these will be grouped by account type
	const otherAccounts = accounts.filter((a) => a.address !== activeAddress);

	// todo: replace this with a real flow
	const [unlockModalOpen, setUnlockModalOpen] = useState(false);
	const handleUnlockAccount = () => {
		setUnlockModalOpen(true);
	};

	const close = () => setUnlockModalOpen(false);

	const handleSelectAccount = async (address: string) => {
		const account = accounts.find((a) => a.address === address);
		if (!account) return;
		if (address !== activeAddress) {
			ampli.switchedAccount({
				toAccountType: account.type,
			});
			await backgroundClient.selectAccount(address);
		}
	};

	return (
		<div className="bg-gradients-graph-cards flex flex-col rounded-xl p-4 gap-5 border border-solid border-hero/10 w-full">
			<Heading variant="heading5" weight="semibold" color="steel-darker">
				Accounts
			</Heading>

			<ToggleGroup.Root
				asChild
				value={activeAddress!}
				type="single"
				onValueChange={handleSelectAccount}
			>
				<>
					<Collapsible title="Current" defaultOpen shade="darker">
						<ToggleGroup.Item asChild value={activeAddress!}>
							<div>
								<AccountListItem
									address={activeAddress!}
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
										<ToggleGroup.Item asChild key={account.address} value={account.address}>
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
