// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cx } from 'class-variance-authority';

import { Button } from '../../shared/ButtonUI';
import { Heading } from '../../shared/heading';
import { Portal } from '../../shared/Portal';
import type { DappPreflightResponse } from './types';
import WarningSvg from './warning.svg';

export type ScamOverlayProps = {
	preflight: DappPreflightResponse;
	onClickBack(): void;
	onClickContinue(): void;
};

function Warning({ title, subtitle }: { title: string; subtitle: string }) {
	return (
		<div className="flex flex-col gap-2 text-center pb-4">
			<Heading variant="heading2" weight="semibold" color="gray-90">
				{title || 'Malicious website'}
			</Heading>
			<div className="flex text-center font-medium text-pBody text-gray-90">
				<div className="font-medium text-pBody text-gray-90">
					{subtitle ||
						'This website has been flagged for malicious behavior. To protect your wallet from potential threats, please return to safety.'}
				</div>
			</div>
		</div>
	);
}

export function ScamOverlay({ preflight, onClickBack, onClickContinue }: ScamOverlayProps) {
	const { block, warnings } = preflight;

	if (!block.enabled && !warnings?.length) return null;

	return (
		<Portal containerId="overlay-portal-container">
			<div
				className={cx(
					'h-full w-full flex flex-col p-4 justify-center items-center gap-4 absolute top-0 left-0 bottom-0 z-50',
					block.enabled ? 'bg-issue-light' : 'bg-warning-light',
				)}
			>
				<WarningSvg />

				{!!block.enabled && <Warning {...preflight.block} />}

				{warnings?.map(({ title, subtitle }, i) => {
					// warnings list won't ever change, index key is fine
					return <Warning key={i} title={title} subtitle={subtitle} />;
				})}

				<div className="flex flex-col gap-2 mt-auto w-full items-stretch">
					<Button variant="primary" text="Return to safety" onClick={onClickBack} />
					{!block.enabled && (
						<Button variant="outlineWarning" text="Proceed" onClick={onClickContinue} />
					)}
				</div>
			</div>
		</Portal>
	);
}
