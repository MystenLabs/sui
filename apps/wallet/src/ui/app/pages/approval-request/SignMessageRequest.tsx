// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/sui.js';
import { useMemo } from 'react';

import { UserApproveContainer } from '../../components/user-approve-container';
import { useAppDispatch } from '../../hooks';
import { respondToTransactionRequest } from '../../redux/slices/transaction-requests';
import { Heading } from '../../shared/heading';
import { Text } from '../../shared/text';
import { type SignMessageApprovalRequest } from '_payloads/transactions/ApprovalRequest';

export type SignMessageRequestProps = {
    request: SignMessageApprovalRequest;
};

export function SignMessageRequest({ request }: SignMessageRequestProps) {
    const message = useMemo(() => {
        return new TextDecoder().decode(fromB64(request.tx.message));
    }, [request.tx.message]);
    const dispatch = useAppDispatch();
    return (
        <UserApproveContainer
            origin={request.origin}
            originFavIcon={request.originFavIcon}
            approveTitle="Sign"
            rejectTitle="Reject"
            onSubmit={(approved) =>
                dispatch(
                    respondToTransactionRequest({
                        txRequestID: request.id,
                        approved,
                        addressForTransaction: request.tx.accountAddress,
                    })
                )
            }
            address={request.tx.accountAddress}
            scrollable
        >
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
                    <Text variant="p2" weight="medium" color="steel-darker">
                        {message}
                    </Text>
                </div>
            </div>
        </UserApproveContainer>
    );
}
