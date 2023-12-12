// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import classNames from 'clsx';
import { ReactNode } from 'react';

import { Spinner } from './Spinner';

export function Button({
	children,
	loading,
	className,
	disabled,
	onClick,
	...props
}: {
	children: ReactNode;
	loading?: boolean;
	className?: string;
	onClick: () => Promise<void> | void;
	disabled?: boolean;
}) {
	return (
		<button
			className={classNames(
				'ease-in-out duration-300 disabled:opacity-30 rounded border py-2 px-4 bg-gray-200',
				className,
			)}
			onClick={onClick}
			disabled={!!disabled}
			{...props}
		>
			{loading ? <Spinner /> : children}
		</button>
	);
}
