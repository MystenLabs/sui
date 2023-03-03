// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Button, type ButtonProps } from './ButtonUI';
import { ModalDialog } from './ModalDialog';

export type ConfirmationModalProps = {
    isOpen: boolean;
    title?: string;
    hint?: string;
    children?: React.ReactNode;
    confirmText?: string;
    confirmStyle?: ButtonProps['variant'];
    cancelText?: string;
    onResponse: (confirmed: boolean) => void;
};

export function ConfirmationModal({
    isOpen,
    title = 'Are you sure?',
    children,
    confirmText = 'Confirm',
    confirmStyle = 'primary',
    cancelText = 'Cancel',
    onResponse,
}: ConfirmationModalProps) {
    return (
        <ModalDialog
            isOpen={isOpen}
            title={title}
            body={children}
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
