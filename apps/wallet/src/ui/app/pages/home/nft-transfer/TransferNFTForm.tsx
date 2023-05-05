// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight16 } from '@mysten/icons';
import { getTransactionDigest, TransactionBlock } from '@mysten/sui.js';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Form, Field, Formik } from 'formik';
import { toast } from 'react-hot-toast';
import { useNavigate } from 'react-router-dom';

import { createValidationSchema } from './validation';
import { useActiveAddress } from '_app/hooks/useActiveAddress';
import { Button } from '_app/shared/ButtonUI';
import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import { Text } from '_app/shared/text';
import { AddressInput } from '_components/address-input';
import { useSigner } from '_hooks';
import { getSignerOperationErrorMessage } from '_src/ui/app/helpers/errorMessages';

export function TransferNFTForm({ objectId }: { objectId: string }) {
    const activeAddress = useActiveAddress();
    const validationSchema = createValidationSchema(
        activeAddress || '',
        objectId
    );
    const signer = useSigner();
    const queryClient = useQueryClient();
    const navigate = useNavigate();
    const transferNFT = useMutation({
        mutationFn: async (to: string) => {
            if (!to || !signer) {
                throw new Error('Missing data');
            }
            const tx = new TransactionBlock();
            tx.transferObjects([tx.object(objectId)], tx.pure(to));

            return signer.signAndExecuteTransactionBlock({
                transactionBlock: tx,
                options: {
                    showInput: true,
                    showEffects: true,
                    showEvents: true,
                },
            });
        },
        onSuccess: (response) => {
            queryClient.invalidateQueries(['object', objectId]);
            queryClient.invalidateQueries(['get-owned-objects']);
            return navigate(
                `/receipt?${new URLSearchParams({
                    txdigest: getTransactionDigest(response),
                    from: 'nfts',
                }).toString()}`
            );
        },
        onError: (error) => {
            toast.error(
                <div className="max-w-xs overflow-hidden flex flex-col">
                    <small className="text-ellipsis overflow-hidden">
                        {getSignerOperationErrorMessage(error)}
                    </small>
                </div>
            );
        },
    });

    return (
        <Formik
            initialValues={{
                to: '',
            }}
            validateOnMount
            validationSchema={validationSchema}
            onSubmit={({ to }) => transferNFT.mutateAsync(to)}
        >
            {({ isValid }) => (
                <Form autoComplete="off" className="h-full">
                    <BottomMenuLayout className="h-full">
                        <Content>
                            <div className="flex gap-2.5 flex-col">
                                <div className="px-2.5 tracking-wider">
                                    <Text
                                        variant="caption"
                                        color="steel-dark"
                                        weight="semibold"
                                    >
                                        Enter Recipient Address
                                    </Text>
                                </div>
                                <div className="w-full flex relative items-center flex-col">
                                    <Field
                                        component={AddressInput}
                                        allowNegative={false}
                                        name="to"
                                        placeholder="Enter Address"
                                    />
                                </div>
                            </div>
                        </Content>
                        <Menu
                            stuckClass="sendCoin-cta"
                            className="w-full px-0 pb-0 mx-0 gap-2.5"
                        >
                            <Button
                                type="submit"
                                variant="primary"
                                loading={transferNFT.isLoading}
                                disabled={!isValid}
                                size="tall"
                                text="Send NFT Now"
                                after={<ArrowRight16 />}
                            />
                        </Menu>
                    </BottomMenuLayout>
                </Form>
            )}
        </Formik>
    );
}
