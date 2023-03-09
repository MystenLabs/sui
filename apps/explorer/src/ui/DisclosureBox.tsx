// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Disclosure } from '@headlessui/react';
import { ChevronDown24 } from '@mysten/icons';
import { cva } from 'class-variance-authority';

import type { ReactNode } from 'react';

export type DisclosureBoxProps = {
    defaultOpen?: boolean;
    title: ReactNode;
    children: ReactNode;
    variant: 'inline' | 'module';
};

const disclosureStyles = cva('', {
    variants: {
        display: {
            inline: '',
            module: 'rounded-lg bg-gray-40',
        },
    },
});

const buttonStyles = cva('flex cursor-pointer select-none', {
    variants: {
        display: {
            inline: 'gap-1 items-center flex pb-3.5 text-hero-dark text-normal',
            module: 'flex-nowrap items-center py-3.75 px-5 justify-between text-gray-90 text-semibold',
        },
    },
});

const panelStyles = cva('px-5 pb-5', {
    variants: {
        display: {
            inline: 'bg-gray-40 rounded-t-lg p-4',
            module: '',
        },
    },
});

const textStyles = cva('text-body', {
    variants: {
        display: {
            inline: 'font-medium',
            module: 'font-semibold',
        },
    },
});

export function DisclosureBox({
    defaultOpen,
    title,
    children,
    variant,
}: DisclosureBoxProps) {
    return (
        <Disclosure
            as="div"
            className={disclosureStyles({ display: variant })}
            defaultOpen={defaultOpen}
        >
            <Disclosure.Button
                as="div"
                className={buttonStyles({ display: variant })}
            >
                <div className={textStyles({ display: variant })}>{title}</div>
                <ChevronDown24 className="-rotate-90 text-steel ui-open:rotate-0" />
            </Disclosure.Button>
            <Disclosure.Panel className={panelStyles({ display: variant })}>
                {children}
            </Disclosure.Panel>
        </Disclosure>
    );
}
