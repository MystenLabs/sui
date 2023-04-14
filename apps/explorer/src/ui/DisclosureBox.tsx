// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Disclosure } from '@headlessui/react';
import { ChevronRight12 } from '@mysten/icons';
import { cva, type VariantProps } from 'class-variance-authority';

import type { ReactNode } from 'react';

const disclosureBoxStyles = cva('', {
    variants: {
        variant: {
            primary: 'bg-gray-40 rounded-lg',
            outline:
                'bg-transparent border border-gray-45 hover:bg-gray-40 hover:border-transparent rounded-2lg',
        },
    },
    defaultVariants: {
        variant: 'primary',
    },
});

export interface DisclosureBoxProps
    extends VariantProps<typeof disclosureBoxStyles> {
    defaultOpen?: boolean;
    title: ReactNode;
    preview?: ReactNode;
    children: ReactNode;
}

export function DisclosureBox({
    defaultOpen,
    title,
    children,
    preview,
    variant,
}: DisclosureBoxProps) {
    return (
        <div className={disclosureBoxStyles({ variant })}>
            <Disclosure defaultOpen={defaultOpen}>
                {({ open }) => (
                    <>
                        <Disclosure.Button
                            as="div"
                            className="flex cursor-pointer flex-nowrap items-center gap-1 px-5 py-3.75"
                        >
                            <div className="flex w-11/12 flex-1 gap-1 text-body font-semibold text-gray-90">
                                {title}
                                {preview && !open ? preview : null}
                            </div>

                            <ChevronRight12 className="text-caption text-steel ui-open:rotate-90" />
                        </Disclosure.Button>
                        <Disclosure.Panel className="px-5 pb-3.75">
                            {children}
                        </Disclosure.Panel>
                    </>
                )}
            </Disclosure>
        </div>
    );
}
