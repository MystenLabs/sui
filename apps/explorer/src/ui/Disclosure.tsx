// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Disclosure as HeadlessDisclosure } from '@headlessui/react';
import { ChevronDown24 } from '@mysten/icons';
import { cva } from 'class-variance-authority';

import type { ReactNode } from 'react';

export type DisclosureProps = {
    defaultOpen?: boolean;
    title: ReactNode;
    children: ReactNode;
    variant: 'inline' | 'module';
};

const disclosureStyles = cva('', {
    variants: {
        display: {
            module: 'rounded-lg bg-gray-40',
            inline: '',
        },
    },
});

const buttonStyles = cva('flex cursor-pointer select-none', {
    variants: {
        display: {
            inline: 'gap-1 items-center text-p1 flex ui-open:pb-3.5 text-hero-dark font-normal',
            module: 'flex-nowrap items-center py-3.75 px-5 justify-between text-body text-gray-90 font-semibold',
        },
    },
});

const panelStyles = cva('', {
    variants: {
        display: {
            inline: 'bg-gray-40 rounded-lg p-5',
            module: 'py-3.75 px-5',
        },
    },
});

export function Disclosure({
    defaultOpen,
    title,
    children,
    variant,
}: DisclosureProps) {
    return (
        <HeadlessDisclosure
            as="div"
            className={disclosureStyles({ display: variant })}
            defaultOpen={defaultOpen}
        >
            <HeadlessDisclosure.Button
                as="div"
                className={buttonStyles({ display: variant })}
            >
                {title}
                <ChevronDown24 className="-rotate-90 text-steel ui-open:rotate-0" />
            </HeadlessDisclosure.Button>
            <HeadlessDisclosure.Panel
                className={panelStyles({ display: variant })}
            >
                {children}
            </HeadlessDisclosure.Panel>
        </HeadlessDisclosure>
    );
}
