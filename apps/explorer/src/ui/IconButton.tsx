// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ButtonOrLink, type ButtonOrLinkProps } from './utils/ButtonOrLink';

import type { FC } from 'react';

export interface IconButtonProps
	extends Omit<ButtonOrLinkProps, 'children' | 'aria-label'>,
		Required<Pick<ButtonOrLinkProps, 'aria-label'>> {
	icon: FC;
}

export function IconButton({ icon: IconComponent, ...props }: IconButtonProps) {
	return (
		<ButtonOrLink
			className="inline-flex cursor-pointer items-center justify-center bg-transparent px-3 py-2 text-steel-dark hover:text-steel-darker active:text-steel disabled:cursor-default disabled:text-gray-60"
			{...props}
		>
			<IconComponent />
		</ButtonOrLink>
	);
}
