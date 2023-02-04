// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
    Copy12 as CopyIcon,
    Image16 as ImageIcon,
    Sender16 as SenderIcon,
    Call16 as CallIcon,
    ChangeEpoch16 as ChangeEpochIcon,
    Publish16 as PublishIcon,
    Tokens32 as PayIcon,
    TransferObject16 as TransferObjectIcon,
    TransferSui16 as TransferSuiIcon,
} from '@mysten/icons';
import { type TransactionKindName } from '@mysten/sui.js';
import toast from 'react-hot-toast';

import { Badge } from './Badge';
import { Heading } from './Heading';

export type PageHeaderType =
    | TransactionKindName
    | 'Address'
    | 'Object'
    | 'Package';

export interface PageHeaderProps {
    title: string;
    subtitle?: string;
    type: PageHeaderType;
    status?: 'success' | 'failure';
}

const TYPE_TO_ICON: Record<string, typeof CallIcon> = {
    Call: CallIcon,
    ChangeEpoch: ChangeEpochIcon,
    Pay: PayIcon,
    // TODO: replace with SUI specific icon if needed
    PaySui: PayIcon,
    PayAllSui: PayIcon,
    Publish: PublishIcon,
    TransferObject: TransferObjectIcon,
    TransferSui: TransferSuiIcon,
    Object: ImageIcon,
    Package: CallIcon,
    Address: () => (
        <SenderIcon
            style={{
                '--icon-primary-color': 'var(--steel)',
                '--icon-secondary-color': 'white',
            }}
        />
    ),
};

const STATUS_TO_TEXT = {
    success: 'Success',
    failure: 'Failure',
};

export function PageHeader({ title, subtitle, type, status }: PageHeaderProps) {
    const Icon = TYPE_TO_ICON[type];

    return (
        <div data-testid="pageheader">
            <div className="mb-3 flex items-center gap-2">
                {Icon && <Icon className="text-steel" />}
                <Heading variant="heading4/semibold" color="steel-darker">
                    {type}
                </Heading>
            </div>
            <div className="flex flex-col gap-2 lg:flex-row">
                <div className="flex min-w-0 items-start gap-2">
                    <div className="min-w-0 break-words">
                        <Heading
                            as="h2"
                            variant="heading2/semibold"
                            color="gray-90"
                            mono
                        >
                            {title}
                        </Heading>
                    </div>
                    <button
                        type="button"
                        onClick={() => {
                            navigator.clipboard.writeText(title);
                            toast.success('Copied!');
                        }}
                        className="m-0 -mt-0.5 flex cursor-pointer items-center justify-center border-none bg-transparent p-0 text-steel"
                    >
                        <span className="sr-only">Copy</span>
                        <CopyIcon className="w-4.5" aria-hidden="true" />
                    </button>
                </div>

                {status && (
                    <div>
                        <Badge variant={status}>{STATUS_TO_TEXT[status]}</Badge>
                    </div>
                )}
            </div>
            {subtitle && (
                <div className="mt-2 break-words">
                    <Heading variant="heading4/semibold" color="gray-75">
                        {subtitle}
                    </Heading>
                </div>
            )}
        </div>
    );
}
