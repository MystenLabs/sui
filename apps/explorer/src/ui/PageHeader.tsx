// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Badge } from './Badge';
import { Heading } from './Heading';
import { ReactComponent as CopyIcon } from './icons/copy.svg';

export interface PageHeaderProps {
    title: string;
    // TODO: Encode fixed set of types?
    type: string;
    status: 'success' | 'failure';
}

const STATUS_TO_TEXT = {
    success: 'Success',
    failure: 'Failure',
};

/**
 * TODO:
 * - Figure out what the PageType can be
 * - Verify Spacing
 */

export function PageHeader({ title, status }: PageHeaderProps) {
    return (
        <div className="flex flex-col gap-3">
            <div className="text-sui-grey-85">
                <Heading variant="heading4" weight="semibold">
                    PageType
                </Heading>
            </div>
            <div className="flex flex-col lg:flex-row gap-2">
                <div className="flex items-center gap-2 min-w-0">
                    <div className="break-words min-w-0">
                        <Heading as="h2" variant="heading2" weight="bold" mono>
                            {title}
                        </Heading>
                    </div>
                    {/* TODO: Extract into component? */}
                    <button
                        onClick={() => {
                            navigator.clipboard.writeText(title);
                        }}
                        className="bg-transparent border-none cursor-pointer p-0 m-0 text-sui-steel flex justify-center items-center"
                    >
                        <CopyIcon />
                    </button>
                </div>

                <div>
                    <Badge variant={status}>{STATUS_TO_TEXT[status]}</Badge>
                </div>
            </div>
        </div>
    );
}
