// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Disclosure } from '@headlessui/react';

import { ReactComponent as ChevronDownIcon } from './icons/chevron_down.svg';

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
        <div className="bg-gray-40 rounded-lg">
            <Disclosure defaultOpen={defaultOpen}>
                <Disclosure.Button
                    as="div"
                    className="py-3.75 flex cursor-pointer flex-nowrap items-center px-5"
                >
                    <div className="text-body text-gray-90 flex-1 font-semibold">
                        {title}
                    </div>
                    <ChevronDownIcon className="text-gray-75 ui-open:rotate-0 -rotate-90" />
                </Disclosure.Button>
                <Disclosure.Panel className="px-5 pb-5">
                    {children}
                </Disclosure.Panel>
            </Disclosure>
        </div>
    );
}
