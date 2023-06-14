// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Disclosure } from '@headlessui/react';
import { ChevronRight12 } from '@mysten/icons';
import {
    formatAddress,
    parseStructTag,
    type SuiEvent,
    type TransactionEvents,
} from '@mysten/sui.js';
import clsx from 'clsx';
import { type ReactNode } from 'react';

import { SyntaxHighlighter } from '~/components/SyntaxHighlighter';
import { CopyToClipboard } from '~/ui/CopyToClipboard';
import { Divider } from '~/ui/Divider';
import { ObjectLink } from '~/ui/InternalLink';
import { Text } from '~/ui/Text';

function EventRow({ title, children }: { title: string; children: ReactNode }) {
    return (
        <div className="flex flex-col gap-2 md:flex-row md:gap-10">
            <dt className="w-full flex-shrink-0 text-pBody font-medium text-steel-darker md:w-28">
                {title}
            </dt>
            <dd className="ml-0 min-w-0 flex-1 break-words break-all leading-none">
                {children}
            </dd>
        </div>
    );
}

function Event({ event, divider }: { event: SuiEvent; divider: boolean }) {
    const { address, module, name } = parseStructTag(event.type);
    const objectLinkLabel = [formatAddress(address), module, name].join('::');

    return (
        <div>
            <div className="flex flex-col gap-3">
                <EventRow title="Type">
                    <Text variant="pBody/medium" color="steel-darker">
                        {objectLinkLabel}
                    </Text>
                </EventRow>

                <EventRow title="Event Emitter">
                    <div className="flex items-center gap-1">
                        <ObjectLink
                            objectId={event.packageId}
                            queryStrings={{ module: event.transactionModule }}
                            label={`${formatAddress(event.packageId)}::${
                                event.transactionModule
                            }`}
                        />
                        <CopyToClipboard
                            color="steel"
                            copyText={event.packageId}
                        />
                    </div>
                </EventRow>

                <Disclosure>
                    {({ open }) => (
                        <>
                            <Disclosure.Button
                                as="div"
                                className="flex cursor-pointer items-center gap-1.5"
                            >
                                <Text
                                    variant="body/semibold"
                                    color="steel-dark"
                                >
                                    {open ? 'Hide' : 'View'} Event Data
                                </Text>

                                <ChevronRight12
                                    className={clsx(
                                        'h-3 w-3 text-steel-dark',
                                        open && 'rotate-90'
                                    )}
                                />
                            </Disclosure.Button>

                            <Disclosure.Panel className="rounded-lg border border-transparent bg-white p-5">
                                <SyntaxHighlighter
                                    code={JSON.stringify(event, null, 2)}
                                    language="json"
                                />
                            </Disclosure.Panel>
                        </>
                    )}
                </Disclosure>
            </div>

            {divider && (
                <div className="my-6">
                    <Divider />
                </div>
            )}
        </div>
    );
}

interface EventsProps {
    events: TransactionEvents;
}

export function Events({ events }: EventsProps) {
    return (
        <div>
            {events.map((event, index) => (
                <Event
                    key={event.type}
                    event={event}
                    divider={index !== events.length - 1}
                />
            ))}
        </div>
    );
}
