// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Button } from '../../shared/ButtonUI';
import { Heading } from '../../shared/heading';
import { Portal } from '../../shared/Portal';
import WarningSvg from './warning.svg';

export type ScamOverlayProps = {
	onDismiss(): void;
	open: boolean;
};

export function ScamOverlay({ open, onDismiss }: ScamOverlayProps) {
	if (!open) return null;
	return (
		<Portal containerId="overlay-portal-container">
			<div className="h-full w-full bg-issue-light flex flex-col p-4 justify-center items-center gap-4 absolute top-0 left-0 bottom-0 z-50">
				<WarningSvg />
				<div className="flex flex-col gap-2 text-center pb-4">
					<Heading variant="heading2" weight="semibold" color="gray-90">
						Malicious website
					</Heading>
					<div className="flex text-center font-medium text-pBody text-gray-90">
						<div className="font-medium text-pBody text-gray-90">
							This website has been flagged for malicious behavior. To protect your wallet from
							potential threats, please return to safety.
						</div>
					</div>
				</div>

				<div className="gap-2 mt-auto w-full items-stretch">
					<Button variant="primary" text="I understand" onClick={onDismiss} />
				</div>
			</div>
		</Portal>
	);
}
