// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Disclosure, Transition } from '@headlessui/react';
import { ChevronDown16, Copy16 } from '@mysten/icons';
import { formatAddress } from '@mysten/sui.js';
import { cx } from 'class-variance-authority';

import { AccountBadge } from '../../AccountBadge';
import { AccountActions } from './AccountActions';
import { type SerializedAccount } from '_src/background/keyring/Account';
import { useCopyToClipboard } from '_src/ui/app/hooks/useCopyToClipboard';
import { Heading } from '_src/ui/app/shared/heading';

export type AccountProps = {
    account: SerializedAccount;
};

export function Account({ account }: AccountProps) {
    const { address, type } = account;
    const copyCallback = useCopyToClipboard(address, {
        copySuccessMessage: 'Address copied',
    });

    return (
        <Disclosure>
            {({ open }) => (
                <div
                    className={cx(
                        'transition flex flex-col flex-nowrap border border-solid rounded-lg',
                        open
                            ? 'bg-gray-40 border-transparent'
                            : 'hover:border-steel border-gray-60'
                    )}
                >
                    <Disclosure.Button
                        as="div"
                        className="flex flex-nowrap items-center p-5 self-stretch cursor-pointer gap-3 group"
                    >
                        <div className="transition flex flex-1 gap-3 justify-start items-center text-steel-dark group-hover:text-steel-darker ui-open:text-steel-darker">
                            <Heading
                                mono
                                weight="semibold"
                                variant="heading6"
                                leading="none"
                            >
                                {formatAddress(address)}
                            </Heading>
                            <AccountBadge accountType={type} />
                        </div>
                        <Copy16
                            onClick={copyCallback}
                            className="transition text-base leading-none text-gray-60 active:text-gray-60 group-hover:text-hero-darkest cursor-pointer"
                        />
                        <ChevronDown16 className="transition text-base leading-none text-gray-60 ui-open:rotate-180 ui-open:text-hero-darkest group-hover:text-hero-darkest" />
                    </Disclosure.Button>
                    <Transition
                        enter="transition duration-100 ease-out"
                        enterFrom="transform opacity-0"
                        enterTo="transform opacity-100"
                        leave="transition duration-75 ease-out"
                        leaveFrom="transform opacity-100"
                        leaveTo="transform opacity-0"
                    >
                        <Disclosure.Panel className="px-5 pb-4">
                            <AccountActions account={account} />
                        </Disclosure.Panel>
                    </Transition>
                </div>
            )}
        </Disclosure>
    );
}
