// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useCallback, useMemo, useState } from 'react';

import { Button } from '../../shared/ButtonUI';
import { DAppInfoCard } from '../DAppInfoCard';

import type { ReactNode } from 'react';

import st from './UserApproveContainer.module.scss';

type UserApproveContainerProps = {
	children: ReactNode | ReactNode[];
	origin: string;
	originFavIcon?: string;
	rejectTitle: string;
	approveTitle: string;
	approveDisabled?: boolean;
	approveLoading?: boolean;
	onSubmit: (approved: boolean) => Promise<void>;
	isWarning?: boolean;
	addressHidden?: boolean;
	address?: string | null;
	scrollable?: boolean;
	blended?: boolean;
};

export function UserApproveContainer({
	origin,
	originFavIcon,
	children,
	rejectTitle,
	approveTitle,
	approveDisabled = false,
	approveLoading = false,
	onSubmit,
	isWarning,
	addressHidden = false,
	address,
	scrollable,
	blended = false,
}: UserApproveContainerProps) {
	const [submitting, setSubmitting] = useState(false);
	const handleOnResponse = useCallback(
		async (allowed: boolean) => {
			setSubmitting(true);
			await onSubmit(allowed);
			setSubmitting(false);
		},
		[onSubmit],
	);

	const parsedOrigin = useMemo(() => new URL(origin), [origin]);

	return (
		<div className={st.container}>
			<div className={cl(st.scrollBody, { [st.scrollable]: scrollable })}>
				<DAppInfoCard
					name={parsedOrigin.host}
					url={origin}
					iconUrl={originFavIcon}
					connectedAddress={!addressHidden && address ? address : undefined}
				/>
				<div className={cl(st.children, { [st.scrollable]: scrollable, [st.blended]: blended })}>
					{children}
				</div>
			</div>
			<div className={st.actionsContainer}>
				<div
					className={cl(st.actions, isWarning && st.flipActions, {
						[st.blended]: blended,
						[st.blurBorder]: blended,
					})}
				>
					<Button
						size="tall"
						variant="warning"
						onClick={() => {
							handleOnResponse(false);
						}}
						disabled={submitting}
						text={rejectTitle}
					/>
					<Button
						// recreate the button when changing the variant to avoid animating to the new styles
						key={`approve_${isWarning}`}
						size="tall"
						variant={isWarning ? 'secondary' : 'primary'}
						onClick={() => {
							handleOnResponse(true);
						}}
						disabled={approveDisabled}
						loading={submitting || approveLoading}
						text={approveTitle}
					/>
				</div>
			</div>
		</div>
	);
}
