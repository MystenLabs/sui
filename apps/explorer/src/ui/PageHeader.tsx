// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type TransactionKindName } from '@mysten/sui.js';
import toast from 'react-hot-toast';

import { Badge } from './Badge';
import { Heading } from './Heading';
import { ReactComponent as CopyIcon } from './icons/copy.svg';
import { ReactComponent as ImageIcon } from './icons/image.svg';
import { ReactComponent as SenderIcon } from './icons/sender.svg';
import { ReactComponent as CallIcon } from './icons/transactions/call.svg';
import { ReactComponent as ChangeEpochIcon } from './icons/transactions/changeEpoch.svg';
import { ReactComponent as PayIcon } from './icons/transactions/pay.svg';
import { ReactComponent as PublishIcon } from './icons/transactions/publish.svg';
import { ReactComponent as TransferObjectIcon } from './icons/transactions/transferObject.svg';
import { ReactComponent as TransferSuiIcon } from './icons/transactions/transferSui.svg';

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

const TYPE_TO_ICON: Record<PageHeaderType, typeof CallIcon> = {
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
                '--icon-primary-color': 'var(--sui-steel)',
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
            <div className="text-sui-grey-85 flex items-center gap-2 mb-3">
                <Icon className="text-sui-steel" />
                <Heading variant="heading4" weight="semibold">
                    {type}
                </Heading>
            </div>
            <div className="flex flex-col lg:flex-row gap-2">
                <div className="flex items-start gap-2 min-w-0">
                    <div className="break-words min-w-0">
                        <Heading as="h2" variant="heading2" weight="bold" mono>
                            {title}
                        </Heading>
                    </div>
                    <button
                        type="button"
                        onClick={() => {
                            navigator.clipboard.writeText(title);
                            toast.success('Copied!');
                        }}
                        className="bg-transparent border-none cursor-pointer p-0 m-0 text-sui-steel flex justify-center items-center -mt-0.5"
                    >
                        <span className="sr-only">Copy</span>
                        <CopyIcon aria-hidden="true" />
                    </button>
                </div>

                {status && (
                    <div>
                        <Badge variant={status}>{STATUS_TO_TEXT[status]}</Badge>
                    </div>
                )}
            </div>
            {subtitle && (
                <div className="text-sui-grey-75 mt-2">
                    <Heading variant="heading4" weight="semibold">
                        {subtitle}
                    </Heading>
                </div>
            )}
        </div>
    );
}
