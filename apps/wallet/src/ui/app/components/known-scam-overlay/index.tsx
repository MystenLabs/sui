// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';

import { Button } from '../../shared/ButtonUI';
import { Heading } from '../../shared/heading';
import { Portal } from '../../shared/Portal';
import { Text } from '../../shared/text';
import WarningSvg from './warning.svg';

export type ScamOverlayProps = {
	onDismiss(): void;
	onContinue(): void;
	open: boolean;
};

export function ScamOverlay({ onContinue, open, onDismiss }: ScamOverlayProps) {
	const [connectAnyway, setConnectAnyway] = useState(false);

	if (!open) return null;
	return (
		<Portal containerId="overlay-portal-container">
			<div className="h-full w-full bg-issue-light flex flex-col p-4 justify-center items-center gap-4 absolute top-0 left-0 bottom-0 z-50">
				<WarningSvg />
				<div className="flex flex-col gap-2 text-center pb-4">
					<Heading variant="heading2" weight="semibold" color="gray-90">
						{connectAnyway
							? 'Engage with potentially malicious site?'
							: 'Malicious activity reported'}
					</Heading>
					<div className="flex text-center font-medium text-pBody text-gray-90">
						{connectAnyway ? (
							<Text variant="pBody" weight="medium" color="gray-90">
								Granting access to your wallet and engaging in transactions could expose your wallet
								and assets to security risks. The potential consequences of engaging with this site
								could be severe.
							</Text>
						) : (
							<div className="font-medium text-pBody text-gray-90">
								This website has been flagged in reports of malicious activity. Extreme caution
								should be used if you choose to{' '}
								<button
									className="cursor-pointer bg-transparent p-0 border-none "
									onClick={() => setConnectAnyway(true)}
								>
									<Text variant="pBody" weight="semibold" color="black">
										proceed
									</Text>
								</button>{' '}
								or engage with this site.
							</div>
						)}
					</div>
				</div>
				<div className="flex flex-col items-center border border-solid border-issue-dark/20 rounded-lg bg-issue-light overflow-hidden">
					<div className="w-full bg-issue-dark p-4 text-center">
						<Heading variant="heading6" color="white">
							Proceeding may result in:
						</Heading>
					</div>

					<div className="px-4">
						<ul className="pl-4">
							<li>
								<Text variant="pBodySmall" color="gray-90" weight="normal">
									Password and/or Secret Recovery Phrase theft resulting in loss of wallet control
								</Text>
							</li>
							<li>
								<Text variant="pBodySmall" color="gray-90" weight="normal">
									Misleading transactions resulting in asset theft
								</Text>
							</li>
							<li>
								<Text variant="pBodySmall" color="gray-90" weight="normal">
									Other malicious actions
								</Text>
							</li>
						</ul>
					</div>
				</div>

				<div className="gap-2 mt-auto w-full items-stretch">
					<Button variant="primary" text="Return to safety" onClick={onDismiss} />
					{connectAnyway && (
						<div className="pt-2.5">
							<Button variant="warning" text="Proceed at your own risk" onClick={onContinue} />
						</div>
					)}
				</div>
			</div>
		</Portal>
	);
}
