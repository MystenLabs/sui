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

// adding a sub title to the disclosure box for disappearing preview
export interface DisclosureBoxProps
    extends VariantProps<typeof disclosureBoxStyles> {
    defaultOpen?: boolean;
    title: ReactNode;
    subTitle?: ReactNode;
    children: ReactNode;
    footer?: ReactNode;
}

export function DisclosureBox({
    defaultOpen,
    title,
    children,
    subTitle,
    variant,
    footer,
}: DisclosureBoxProps) {
    return (
        <div className={disclosureBoxStyles({ variant })}>
            <Disclosure defaultOpen={defaultOpen}>
                {({ open }) => (
                    <>
                        <Disclosure.Button
                            as="div"
                            className="flex cursor-pointer flex-nowrap items-center py-3.75 px-5"
                        >
                            <div className="flex flex-1 text-body font-semibold text-gray-90">
                                {title}
                                {subTitle && !open ? subTitle : null}
                            </div>

                            <ChevronRight12 className="text-caption text-steel ui-open:rotate-90" />
                        </Disclosure.Button>
                        <Disclosure.Panel className="px-5 pb-3.75">
                            {children}
                        </Disclosure.Panel>

                        {footer && open ? (
                            <Disclosure.Panel className="mx-5 border-t border-gray-45 py-3.75">
                                {footer}
                            </Disclosure.Panel>
                        ) : null}
                    </>
                )}
            </Disclosure>
        </div>
    );
}
