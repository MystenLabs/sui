// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ReactNode } from 'react';

import { VerifyLedgerConnectionStatus } from './VerifyLedgerConnectionStatus';
import { BadgeLabel } from '../../BadgeLabel';
import { useNextMenuUrl } from '../hooks';
import { type SerializedUIAccount } from '_src/background/accounts/Account';
import { isImportedAccountSerializedUI } from '_src/background/accounts/ImportedAccount';
import { isLedgerAccountSerializedUI } from '_src/background/accounts/LedgerAccount';
import { isMnemonicSerializedUiAccount } from '_src/background/accounts/MnemonicAccount';
import { isQredoAccountSerializedUI } from '_src/background/accounts/QredoAccount';
import { isZkAccountSerializedUI } from '_src/background/accounts/zk/ZkAccount';
import { Button } from '_src/ui/app/shared/ButtonUI';
import { Link } from '_src/ui/app/shared/Link';

export type AccountActionsProps = {
	account: SerializedUIAccount;
};

export function AccountActions({ account }: AccountActionsProps) {
	const exportAccountUrl = useNextMenuUrl(true, `/export/${account.address}`);
	const recoveryPassphraseUrl = useNextMenuUrl(true, '/recovery-passphrase');

	let actionContent: ReactNode | null = null;
	if (isLedgerAccountSerializedUI(account)) {
		actionContent = (
			<div>
				<VerifyLedgerConnectionStatus
					accountAddress={account.address}
					derivationPath={account.derivationPath}
				/>
			</div>
		);
	} else if (isImportedAccountSerializedUI(account)) {
		actionContent = (
			<div>
				<Link text="Export Private Key" to={exportAccountUrl} color="heroDark" weight="medium" />
			</div>
		);
	} else if (isMnemonicSerializedUiAccount(account)) {
		actionContent = (
			<div className="flex flex-col gap-2 w-full">
				<Button
					variant="secondary"
					text="Export Private Key"
					to={exportAccountUrl}
					color="heroDark"
					disabled
				/>
				<Button
					variant="secondary"
					text="Export Passphrase"
					to={recoveryPassphraseUrl}
					color="heroDark"
					disabled
				/>
			</div>
		);
	} else if (isQredoAccountSerializedUI(account)) {
		actionContent = account.labels?.length
			? account.labels.map(({ name, value }) => <BadgeLabel label={value} key={name} />)
			: null;
	} else if (isZkAccountSerializedUI(account)) {
		actionContent = null;
	} else {
		throw new Error(`Encountered unknown account type`);
	}

	return <div className="flex items-center flex-1 gap-4 pb-1 overflow-x-auto">{actionContent}</div>;
}
