// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { forwardRef } from 'react';

import type { ComponentProps } from 'react';

export interface LabelProps
    extends Omit<ComponentProps<'label'>, 'ref' | 'className'> {
    label: string;
}

export const Label = forwardRef<HTMLLabelElement, LabelProps>(
    ({ label, children, ...labelProps }, ref) => {
        return (
            <label
                ref={ref}
                {...labelProps}
                className="flex flex-col flex-nowrap items-stretch gap-2.5"
            >
                <div className="text-bodySmall font-medium text-steel-darker ml-2.5">
                    {label}
                </div>
                {children ? (
                    <div className="flex flex-col flex-nowrap">{children}</div>
                ) : null}
            </label>
        );
    }
);
