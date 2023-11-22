// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	zkLoginProviderDataMap,
	type ZkLoginProvider,
} from '_src/background/accounts/zklogin/providers';
import { isZkLoginAccountSerializedUI } from '_src/background/accounts/zklogin/ZkLoginAccount';
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

const providerToName: Record<ZkLoginProvider, string> = {
	google: 'Google',
	facebook: 'Facebook',
	twitch: 'Twitch',
	kakao: 'kakao',
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
		isZkLoginAccountSerializedUI(activeAccount) &&
		!activeAccount.warningAcknowledged
	) {
		const providerData = zkLoginProviderDataMap[activeAccount.provider];
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
							loading={warningMutation.isPending}
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
