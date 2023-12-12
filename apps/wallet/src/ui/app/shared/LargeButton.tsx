// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import LoadingIndicator from '_components/loading/LoadingIndicator';
import clsx from 'clsx';
import { forwardRef, type ReactNode, type Ref } from 'react';

import { ButtonOrLink, type ButtonOrLinkProps } from './utils/ButtonOrLink';

function Decorator({ disabled, children }: { disabled?: boolean; children: ReactNode }) {
	return (
		<div
			className={clsx(
				'text-heading2 bg-transparent text-center flex justify-center',
				disabled ? 'text-steel' : 'text-hero-dark group-hover:text-hero',
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
	spacing?: string;
	center?: boolean;
	disabled?: boolean;
	primary?: boolean;
}

export const LargeButton = forwardRef(
	(
		{
			top,
			before,
			after,
			center,
			spacing,
			loading,
			disabled,
			children,
			primary,
			className,
			...otherProps
		}: LargeButtonProps,
		ref: Ref<HTMLAnchorElement | HTMLButtonElement>,
	) => {
		return (
			<ButtonOrLink
				ref={ref}
				{...otherProps}
				className={clsx(
					'group border border-solid border-transparent flex rounded-md items-center py-2 px-8 justify-between no-underline',
					disabled ? 'bg-hero-darkest/5 pointer-events-none' : 'bg-white/80 hover:border-sui/10',
					primary ? '!bg-sui-primaryBlue2023' : '',
					spacing === 'sm' && '!p-3',
					className,
				)}
			>
				{loading && (
					<div className="p-2 w-full flex justify-center items-center h-full">
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
									'text-bodySmall font-semibold',
									disabled ? 'text-steel' : 'text-hero-dark group-hover:text-hero',
									primary ? '!text-white' : '',
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
