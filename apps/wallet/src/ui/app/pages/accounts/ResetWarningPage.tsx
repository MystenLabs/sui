// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';
import { useNavigate } from 'react-router-dom';

import { AccountListItem } from '../../components/accounts/AccountListItem';
import { useAccountGroups } from '../../hooks/useAccountGroups';
import { Heading } from '../../shared/heading';
import { Text } from '../../shared/text';
import { Button } from '_app/shared/ButtonUI';

function AccountGroupHeader({ text }: { text: string }) {
	return (
		<div className="flex items-center gap-2 w-full bg-transparent border-none p-0 cursor-pointer group">
			<div className="text-captionSmall font-semibold uppercase text-steel-darker">{text}</div>
			<div className="h-px flex-1 bg-gray-45 bg-steel" />
		</div>
	);
}

export function ResetWarningPage() {
	const navigate = useNavigate();
	const accountGroups = useAccountGroups();
	const accounts = accountGroups.list();

	const passphraseAccounts = useMemo(
		() => accounts.filter((a) => a.type === 'mnemonic-derived') || [],
		[accounts],
	);

	const importedAccounts = useMemo(
		() => accounts.filter((a) => a.type === 'imported') || [],
		[accounts],
	);

	return (
		<div className="rounded-20 bg-sui-lightest shadow-wallet-content flex flex-col items-center px-6 py-10 overflow-auto w-popup-width h-popup-height">
			<div className="flex flex-col items-center gap-2 text-center">
				<Heading variant="heading1" color="gray-90" as="h1" weight="bold">
					Reset Account Password
				</Heading>
				<Text variant="pBody" color="gray-90">
					To ensure account security, the following accounts not associated with the passphrase will
					be removed as part of the password reset process.
				</Text>
			</div>
			<div className="flex flex-col flex-1 overflow-auto mt-5 mb-10 bg-hero-darkest bg-opacity-5 w-full px-4 py-6 gap-8 rounded-lg">
				{passphraseAccounts.length > 0 && (
					<div className="flex flex-col gap-4">
						<AccountGroupHeader text="Passphrase accounts" />
						{passphraseAccounts.map((account) => {
							return <AccountListItem account={account} showLocked={false} />;
						})}
					</div>
				)}

				{importedAccounts.length > 0 && (
					<div className="flex flex-col gap-4">
						<AccountGroupHeader text="Imported accounts" />
						{importedAccounts.map((account) => {
							return <AccountListItem account={account} showLocked={false} />;
						})}
					</div>
				)}
			</div>
			<div className="flex w-full gap-3">
				<Button variant="outline" size="tall" text="Back" onClick={() => navigate(-1)} />
				<Button
					type="submit"
					variant="primary"
					size="tall"
					text="Continue"
					onClick={() =>
						navigate('/accounts/protect-account?accountType=import-mnemonic&reset=true')
					}
				/>
			</div>
		</div>
	);
}
