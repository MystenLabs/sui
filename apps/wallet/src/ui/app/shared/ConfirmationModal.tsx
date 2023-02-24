// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Button, type ButtonProps } from './ButtonUI';
import { ModalDialog } from './ModalDialog';
import { Text } from './text';

export type ConfirmationModalProps = {
    isOpen: boolean;
    title?: string;
    hint?: string;
    confirmText?: string;
    confirmStyle?: ButtonProps['variant'];
    cancelText?: string;
    onResponse: (confirmed: boolean) => void;
};

export function ConfirmationModal({
    isOpen,
    title = 'Are you sure?',
    hint,
    confirmText = 'Confirm',
    confirmStyle = 'primary',
    cancelText = 'Cancel',
    onResponse,
}: ConfirmationModalProps) {
    return (
        <ModalDialog
            isOpen={isOpen}
            title={title}
            body={
                hint ? (
                    <div className="break-words text-center">
                        <Text variant="p2" color="steel-dark" weight="normal">
                            {hint}
                        </Text>
                    </div>
                ) : null
            }
            onClose={() => {
                onResponse(false);
            }}
            footer={
                <div className="flex flex-row self-center gap-3">
                    <div>
                        <Button
                            variant="outline"
                            text={cancelText}
                            onClick={() => onResponse(false)}
                        />
                    </div>
                    <div>
                        <Button
                            variant={confirmStyle}
                            text={confirmText}
                            onClick={() => onResponse(true)}
                        />
                    </div>
                </div>
            }
        />
    );
}
