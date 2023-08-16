// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';

import { HideShowDisplayBox } from '_components/HideShowDisplayBox';
import Alert from '_components/alert';
import { MenuLayout } from '_components/menu/content/MenuLayout';
import { useNextMenuUrl } from '_components/menu/hooks';
import { PasswordInputDialog } from '_src/ui/app/components/PasswordInputDialog';

export function RecoveryPassphrase() {
	const [passwordConfirmed, _setPasswordConfirmed] = useState(false);
	const [mnemonic, _setMnemonic] = useState<string[] | null>(null);
	const accountsUrl = useNextMenuUrl(true, '/accounts');

	if (!passwordConfirmed) {
		return (
			<div className="flex flex-col px-5 pt-10 max-h-popup-height flex-grow">
				<PasswordInputDialog
					showArrowIcon
					title="Export Recovery Passphrase"
					continueLabel="Continue"
					onPasswordVerified={async () => {
						throw new Error('Not implemented yet');
					}}
					showBackButton
				/>
			</div>
		);
	}

	return (
		<MenuLayout title="Your Recovery Passphrase" back={accountsUrl}>
			<div className="flex flex-col gap-3 min-w-0">
				<Alert>
					<div className="break-normal">Do not share your Recovery Passphrase!</div>
					<div className="break-normal">
						It provides full control of all accounts derived from it.
					</div>
				</Alert>

				{mnemonic && (
					<HideShowDisplayBox value={mnemonic} copiedMessage="Recovery Passphrase copied" />
				)}
			</div>
		</MenuLayout>
	);
}
