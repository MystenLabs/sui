// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from 'classnames';
import { forwardRef, type ReactNode, type Ref } from 'react';

import { ButtonOrLink, type ButtonOrLinkProps } from './utils/ButtonOrLink';
import LoadingIndicator from '_components/loading/LoadingIndicator';

function Decorator({ disabled, children }: { disabled?: boolean; children: ReactNode }) {
	return (
		<div
			className={clsx(
				'text-heading2 bg-transparent text-center',
				disabled ? 'text-gray-60' : 'text-hero-dark group-hover:text-hero',
			)}
		>
			{children}
		</div>
	);
}

interface LargeButtonProps extends ButtonOrLinkProps {
	children: ReactNode;
	loading?: boolean;
	before?: ReactNode;
	after?: ReactNode;
	top?: ReactNode;
	center?: boolean;
	disabled?: boolean;
}

export const LargeButton = forwardRef(
	(
		{ top, before, after, center, loading, disabled, children, ...otherProps }: LargeButtonProps,
		ref: Ref<HTMLAnchorElement | HTMLButtonElement>,
	) => {
		return (
			<ButtonOrLink
				ref={ref}
				{...otherProps}
				className={clsx(
					'group border border-solid border-transparent flex rounded-2xl items-center w-full p-3.75 justify-between no-underline',
					disabled ? 'bg-gray-40' : 'bg-sui/10 hover:shadow-drop hover:border-sui/10',
				)}
			>
				{loading && (
					<div className="p-2 w-full flex items-center h-full">
						<LoadingIndicator />
					</div>
				)}
				{!loading && (
					<div className={clsx('flex items-center w-full gap-2.5', center && 'justify-center')}>
						{before && <Decorator disabled={disabled}>{before}</Decorator>}
						<div className="flex flex-col">
							{top && <Decorator disabled={disabled}>{top}</Decorator>}
							<div
								className={clsx(
									'text-body font-semibold',
									disabled ? 'text-gray-60' : 'text-hero-dark group-hover:text-hero',
								)}
							>
								{children}
							</div>
						</div>
						{after && (
							<div className="ml-auto">
								<Decorator disabled={disabled}>{after}</Decorator>
							</div>
						)}
					</div>
				)}
			</ButtonOrLink>
		);
	},
);
