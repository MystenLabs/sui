// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ReactNode } from 'react';

export interface CardProps {
    children: ReactNode;
}

export function Card({ children }: CardProps) {
    return <div className="bg-sui-grey-40 rounded-lg p-7">{children}</div>;
}
