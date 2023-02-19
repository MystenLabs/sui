// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Copy16, EyeClose16, EyeOpen16 } from '@mysten/icons';
import { useState } from 'react';

import { useCopyToClipboard } from '../hooks/useCopyToClipboard';
import { Button } from '../shared/ButtonUI';
import { Link } from '../shared/Link';
import { Text } from '../shared/text';

export type HideShowDisplayBoxProps = {
    value: string;
    copiedMessage?: string;
};

export function HideShowDisplayBox({
    value,
    copiedMessage,
}: HideShowDisplayBoxProps) {
    const [valueHidden, setValueHidden] = useState(true);
    const copyCallback = useCopyToClipboard(value, {
        copySuccessMessage: copiedMessage,
    });
    return (
        <div className="flex flex-col flex-nowrap items-stretch gap-2 bg-white border border-solid border-gray-60 rounded-lg overflow-hidden py-4 px-5">
            <div className="break-all">
                {valueHidden ? (
                    <div className="flex flex-col gap-1.5">
                        <div className="h-3.5 bg-gray-40 rounded-md" />
                        <div className="h-3.5 bg-gray-40 rounded-md" />
                        <div className="h-3.5 bg-gray-40 rounded-md w-1/2" />
                    </div>
                ) : (
                    <Text variant="p1" weight="medium" color="steel-darker">
                        {value}
                    </Text>
                )}
            </div>
            <div className="flex flex-row flex-nowrap items-center justify-between">
                <div>
                    <Link
                        color="heroDark"
                        weight="medium"
                        text="Copy"
                        before={<Copy16 />}
                        onClick={copyCallback}
                    />
                </div>
                <div>
                    <Button
                        variant="plain"
                        size="tiny"
                        text={valueHidden ? <EyeClose16 /> : <EyeOpen16 />}
                        onClick={() => setValueHidden((v) => !v)}
                    />
                </div>
            </div>
        </div>
    );
}
