// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Disclosure } from '@headlessui/react';
import { ChevronRight12 } from '@mysten/icons';
import {
    formatAddress,
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

function formatTypeName(typeName: string) {
    const split = typeName.split('<');
    if (split.length <= 1) {
        return split[0];
    }

    return `${split[0]}<...>`;
}

function formatType(type: string) {
    const split = type.split('::');
    const p0 = formatAddress(split[0]);
    const p1 = split[1];
    const p2 = formatTypeName(split.slice(2).join('::'));

    return [p0, p1, p2].join('::');
}

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

function Event({ event }: { event: SuiEvent }) {
    return (
        <div>
            <div className="flex flex-col gap-3">
                <EventRow title="Type">
                    <div className="flex gap-1">
                        <ObjectLink
                            objectId={`${event.packageId}?module=${event.transactionModule}`}
                            label={formatType(event.type)}
                        />
                        <CopyToClipboard copyText={event.type} />
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
                                    View Event Data
                                </Text>

                                <ChevronRight12
                                    className={clsx(
                                        'h-3 w-3 text-steel-dark',
                                        open && 'rotate-90'
                                    )}
                                />
                            </Disclosure.Button>

                            <Disclosure.Panel>
                                <SyntaxHighlighter
                                    code={JSON.stringify(event, null, 2)}
                                    language="json"
                                />
                            </Disclosure.Panel>
                        </>
                    )}
                </Disclosure>
            </div>

            <div className="my-6">
                <Divider />
            </div>
        </div>
    );
}

interface EventsProps {
    events: TransactionEvents;
}

export function Events({ events }: EventsProps) {
    return (
        <div>
            {events.map((event, idx) => (
                <Event key={idx} event={event} />
            ))}
        </div>
    );
}
