// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ReactNode } from 'react';

import { VerifyLedgerConnectionStatus } from './VerifyLedgerConnectionStatus';
import { BadgeLabel } from '../../BadgeLabel';
import { useNextMenuUrl } from '../hooks';
import { AccountType, type SerializedAccount } from '_src/background/keyring/Account';
import { Button } from '_src/ui/app/shared/ButtonUI';
import { Link } from '_src/ui/app/shared/Link';

export type AccountActionsProps = {
	account: SerializedAccount;
};

export function AccountActions({ account }: AccountActionsProps) {
	const exportAccountUrl = useNextMenuUrl(true, `/export/${account.address}`);
	const recoveryPassphraseUrl = useNextMenuUrl(true, '/recovery-passphrase');

	let actionContent: ReactNode | null = null;
	switch (account.type) {
		case AccountType.LEDGER:
			actionContent = (
				<div>
					<VerifyLedgerConnectionStatus
						accountAddress={account.address}
						derivationPath={account.derivationPath}
					/>
				</div>
			);
			break;
		case AccountType.IMPORTED:
			actionContent = (
				<div>
					<Link text="Export Private Key" to={exportAccountUrl} color="heroDark" weight="medium" />
				</div>
			);
			break;
		case AccountType.DERIVED:
			actionContent = (
				<div className="flex flex-col gap-2 w-full">
					{/* <Button
						variant="secondary"
						text="Export Private Key"
						to={exportAccountUrl}
						color="heroDark"
					/> */}
					<Button
						variant="secondary"
						text="Export Passphrase"
						to={recoveryPassphraseUrl}
						color="heroDark"
					/>
				</div>
			);
			break;
		case AccountType.QREDO:
			actionContent = account.labels?.length
				? account.labels.map(({ name, value }) => <BadgeLabel label={value} key={name} />)
				: null;
			break;
		default:
			throw new Error(`Encountered unknown account type`);
	}

	return <div className="flex items-center flex-1 gap-4 pb-1 overflow-x-auto">{actionContent}</div>;
}
