// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Filter16, Plus12, SocialGoogle24 } from '@mysten/icons';
import * as ToggleGroup from '@radix-ui/react-toggle-group';
import { useState } from 'react';
import { UnlockAccountModal } from './UnlockAccountModal';
import { AccountListItem } from '../../components/accounts/AccountListItem';
import { useActiveAddress } from '../../hooks';
import { Heading } from '../../shared/heading';

import { ampli } from '_src/shared/analytics/ampli';
import { useAccounts } from '_src/ui/app/hooks/useAccounts';
import { useBackgroundClient } from '_src/ui/app/hooks/useBackgroundClient';
import { Link } from '_src/ui/app/shared/Link';
import { Collapsible } from '_src/ui/app/shared/collapse';

export function AccountsList() {
	const activeAddress = useActiveAddress();
	const accounts = useAccounts();
	const backgroundClient = useBackgroundClient();

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
		<div className="bg-gradients-graph-cards flex flex-col rounded-xl p-4 gap-5 border border-solid border-hero/10">
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
					<Collapsible title="Current" defaultOpen>
						<AccountListItem
							icon={<SocialGoogle24 />}
							address={activeAddress!}
							handleUnlockAccount={handleUnlockAccount}
						/>
					</Collapsible>

					<Collapsible title="Switch To">
						<div className="flex flex-col gap-3">
							{accounts
								.filter((account) => account.address !== activeAddress)
								.map((account) => {
									return (
										<AccountListItem
											key={account.address}
											icon={<SocialGoogle24 />}
											address={account.address}
											handleUnlockAccount={handleUnlockAccount}
										/>
									);
								})}
						</div>
					</Collapsible>
				</>
			</ToggleGroup.Root>

			<div className="flex justify-between">
				<div className="flex gap-1 uppercase bg-none rounded-sm text-steel-darker hover:bg-white/60 p-1 items-center justify-center">
					<Link
						before={<Filter16 />}
						color="steelDarker"
						weight="semibold"
						size="captionSmall"
						to="/accounts/manage"
						text="Manage"
					/>
				</div>

				<div className="flex gap-1 uppercase bg-none rounded-sm text-steel-darker hover:bg-white/60 p-1 items-center">
					<Link
						before={<Plus12 />}
						color="steelDarker"
						weight="semibold"
						size="captionSmall"
						to="/accounts/add-account"
						text="Add"
					/>
				</div>
			</div>
			{unlockModalOpen ? <UnlockAccountModal onClose={close} onConfirm={close} /> : null}
		</div>
	);
}
