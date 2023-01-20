// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Disclosure } from '@headlessui/react';
import { ChevronDown16 as ChevronDownIcon } from '@mysten/icons';

import type { ReactNode } from 'react';

export type DisclosureBoxProps = {
    defaultOpen?: boolean;
    title: ReactNode;
    children: ReactNode;
};

export function DisclosureBox({
    defaultOpen,
    title,
    children,
}: DisclosureBoxProps) {
    return (
        <div className="rounded-lg bg-gray-40">
            <Disclosure defaultOpen={defaultOpen}>
                <Disclosure.Button
                    as="div"
                    className="flex cursor-pointer flex-nowrap items-center py-3.75 px-5"
                >
                    <div className="flex-1 text-body font-semibold text-gray-90">
                        {title}
                    </div>
                    <ChevronDownIcon className="-rotate-90 text-gray-75 ui-open:rotate-0" />
                </Disclosure.Button>
                <Disclosure.Panel className="px-5 pb-5">
                    {children}
                </Disclosure.Panel>
            </Disclosure>
        </div>
    );
}
