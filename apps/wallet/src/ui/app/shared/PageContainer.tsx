// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ReactNode } from 'react';

import PageTitle from '_app/shared/PageTitle';

interface Props {
    children: ReactNode;
}

export function PageContainer({ title, children }: Props & { title: string }) {
    return (
        <div className="flex flex-col h-full">
            <PageTitle title={title} />
            {children}
        </div>
    );
}

export function PageContent({ children }: Props) {
    return (
        <div className="mt-5 flex-grow overflow-y-auto px-5 -mx-5">
            {children}
        </div>
    );
}
