// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/sui.js';
import { useMemo } from 'react';

import { UserApproveContainer } from '../../components/user-approve-container';
import { useAppDispatch, useSigner } from '../../hooks';
import { respondToTransactionRequest } from '../../redux/slices/transaction-requests';
import { Heading } from '../../shared/heading';
import { PageMainLayoutTitle } from '../../shared/page-main-layout/PageMainLayoutTitle';
import { Text } from '../../shared/text';
import { type SignMessageApprovalRequest } from '_payloads/transactions/ApprovalRequest';

export type SignMessageRequestProps = {
    request: SignMessageApprovalRequest;
};

export function SignMessageRequest({ request }: SignMessageRequestProps) {
    const { message, type } = useMemo(() => {
        const messageBytes = fromB64(request.tx.message);
        let message: string = request.tx.message;
        let type: 'utf8' | 'base64' = 'base64';
        try {
            message = new TextDecoder('utf8', { fatal: true }).decode(
                messageBytes
            );
            type = 'utf8';
        } catch (e) {
            // do nothing
        }
        return {
            message,
            type,
        };
    }, [request.tx.message]);

    const signer = useSigner(request.tx.accountAddress);
    const dispatch = useAppDispatch();

    return (
        <UserApproveContainer
            origin={request.origin}
            originFavIcon={request.originFavIcon}
            approveTitle="Sign"
            rejectTitle="Reject"
            onSubmit={async (approved) => {
                await dispatch(
                    respondToTransactionRequest({
                        txRequestID: request.id,
                        approved,
                        signer,
                    })
                );
            }}
            address={request.tx.accountAddress}
            scrollable
        >
            <PageMainLayoutTitle title="Sign Message" />
            <div className="flex flex-col flex-nowrap items-stretch border border-solid border-gray-50 rounded-15 overflow-y-auto overflow-x-hidden">
                <div className="sticky top-0 bg-white p-5 pb-2.5">
                    <Heading
                        variant="heading6"
                        color="gray-90"
                        weight="semibold"
                        truncate
                    >
                        Message You Are Signing
                    </Heading>
                </div>
                <div className="px-5 pb-5 break-words">
                    <Text
                        variant="pBodySmall"
                        weight="medium"
                        color="steel-darker"
                        mono={type === 'base64'}
                    >
                        {message}
                    </Text>
                </div>
            </div>
        </UserApproveContainer>
    );
}
