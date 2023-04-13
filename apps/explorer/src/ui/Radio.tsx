// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { RadioGroup as HeadlessRadioGroup } from '@headlessui/react';
import { type ReactNode } from 'react';

import { type ExtractProps } from '~/ui/types';

export type RadioGroupProps = ExtractProps<typeof HeadlessRadioGroup> & {
    children: ReactNode;
    ariaLabel: string;
};

export function RadioGroup({ ariaLabel, children, ...props }: RadioGroupProps) {
    return (
        <HeadlessRadioGroup {...props}>
            <HeadlessRadioGroup.Label role="" className="sr-only">
                {ariaLabel}
            </HeadlessRadioGroup.Label>
            {children}
        </HeadlessRadioGroup>
    );
}

export type RadioOptionProps = ExtractProps<
    typeof HeadlessRadioGroup.Option
> & {
    label?: string;
};

export function RadioOption({ label, children, ...props }: RadioOptionProps) {
    return (
        <HeadlessRadioGroup.Option
            className="flex cursor-pointer flex-col rounded-md border border-transparent bg-white text-steel-dark hover:text-steel-darker ui-checked:border-steel ui-checked:text-hero-dark"
            {...props}
        >
            {label && (
                <HeadlessRadioGroup.Label className="cursor-pointer px-2 py-1 text-captionSmall font-semibold">
                    {label}
                </HeadlessRadioGroup.Label>
            )}
        </HeadlessRadioGroup.Option>
    );
}
