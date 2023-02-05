// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';

import type { ReactNode } from 'react';

export function ReceiptCardBg({
    status,
    children,
}: {
    status: boolean;
    children: ReactNode;
}) {
    return (
        <div
            className={cl(
                "p-5 pb-0 rounded-t-lg flex flex-col item-center after:content-[''] after:w-[320px] after:h-5 after:ml-[-20px] after:top-4 after:-mt-6 after:relative divide-y divide-solid divide-steel/20 divide-x-0 gap-3.5",
                status
                    ? "bg-success-light after:bg-[url('_assets/images/receipt_bottom.svg')]"
                    : "bg-issue-light after:bg-[url('_assets/images/receipt_bottom_red.svg')]"
            )}
        >
            {children}
        </div>
    );
}
