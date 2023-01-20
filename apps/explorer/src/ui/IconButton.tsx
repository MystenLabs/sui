// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { X12 as CloseIcon } from '@mysten/icons';

import { ButtonOrLink, type ButtonOrLinkProps } from './utils/ButtonOrLink';

import type { FC } from 'react';

type IconType = 'x';
const iconTypeToIcon: Record<IconType, FC> = {
    x: CloseIcon,
};

export interface IconButtonProps
    extends Omit<ButtonOrLinkProps, 'children' | 'aria-label'>,
        Required<Pick<ButtonOrLinkProps, 'aria-label'>> {
    icon: IconType;
}

export function IconButton({ icon, ...props }: IconButtonProps) {
    const IconComponent = iconTypeToIcon[icon];
    return (
        <ButtonOrLink
            className="inline-flex cursor-pointer items-center justify-center border-0 bg-transparent px-3 py-2 text-steel-dark hover:text-steel-darker active:text-steel disabled:cursor-default disabled:text-gray-60"
            {...props}
        >
            <IconComponent />
        </ButtonOrLink>
    );
}
