// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { zkProviderDataMap, type ZkProvider } from '_src/background/accounts/zk/providers';
import { isZkAccountSerializedUI } from '_src/background/accounts/zk/ZkAccount';
import { type MethodPayload } from '_src/shared/messaging/messages/payloads/MethodPayload';
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from '_src/ui/app/shared/Dialog';
import { useMutation } from '@tanstack/react-query';
import toast from 'react-hot-toast';

import { useActiveAccount } from '../../hooks/useActiveAccount';
import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { Button } from '../../shared/ButtonUI';
import { Link } from '../../shared/Link';

const providerToName: Record<ZkProvider, string> = {
	google: 'Google',
	facebook: 'Facebook',
	twitch: 'Twitch',
};

export function ZkLoginAccountWarningModal() {
	const activeAccount = useActiveAccount();
	const backgroundClient = useBackgroundClient();
	const warningMutation = useMutation({
		mutationKey: ['acknowledge-zk-login-warning'],
		mutationFn: (args: MethodPayload<'acknowledgeZkLoginWarning'>['args']) =>
			backgroundClient.acknowledgeZkLoginWarning(args),
	});
	if (
		activeAccount &&
		isZkAccountSerializedUI(activeAccount) &&
		!activeAccount.warningAcknowledged
	) {
		const providerData = zkProviderDataMap[activeAccount.provider];
		return (
			<Dialog open>
				<DialogContent onPointerDownOutside={(e) => e.preventDefault()} background="avocado">
					<DialogHeader>
						<DialogTitle className="text-hero-darkest">
							<div>Turn on 2FA.</div>
							<div>Protect Your Assets.</div>
						</DialogTitle>
					</DialogHeader>
					<DialogDescription className="text-center text-steel-darker">
						Your {providerToName[activeAccount.provider]} Account now gives access to your Sui
						Wallet. To help safeguard your assets, we strongly recommend you enable 2FA.
						{providerData.mfaLink ? (
							<>
								{' '}
								<span className="inline-block">
									<Link color="heroDark" href={providerData.mfaLink} text="Visit this link" />
								</span>{' '}
								to find out how to set this up.
							</>
						) : null}
					</DialogDescription>
					<DialogFooter>
						<Button
							text="I understand"
							loading={warningMutation.isLoading}
							onClick={() =>
								warningMutation.mutate(
									{ accountID: activeAccount.id },
									{
										onError: (e) => {
											toast.error((e as Error)?.message || 'Something went wrong');
										},
									},
								)
							}
						/>
					</DialogFooter>
				</DialogContent>
			</Dialog>
		);
	}
	return null;
}
