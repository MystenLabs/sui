// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight16 } from '@mysten/icons';
import { getTransactionDigest, SUI_TYPE_ARG } from '@mysten/sui.js';
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
import Alert from '_components/alert';
import Loading from '_components/loading';
import { useGetCoinBalance, useSigner } from '_hooks';
import { DEFAULT_NFT_TRANSFER_GAS_FEE } from '_redux/slices/sui-objects/Coin';
import { useGasBudgetInMist } from '_src/ui/app/hooks/useGasBudgetInMist';

export function TransferNFTForm({ objectId }: { objectId: string }) {
    const activeAddress = useActiveAddress();
    const validationSchema = createValidationSchema(
        activeAddress || '',
        objectId
    );
    const { data: coinBalance, isLoading: loadingBalances } = useGetCoinBalance(
        SUI_TYPE_ARG,
        activeAddress
    );
    const signer = useSigner();
    const queryClient = useQueryClient();
    const navigate = useNavigate();
    const transferNFT = useMutation({
        mutationFn: (to: string) => {
            if (!to || !signer || isInsufficientGas) {
                throw new Error('Missing data');
            }
            return signer.transferObject({
                recipient: to,
                objectId,
                gasBudget: DEFAULT_NFT_TRANSFER_GAS_FEE,
            });
        },

        onSuccess: (response) => {
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
                    {error instanceof Error ? (
                        <small className="text-ellipsis overflow-hidden">
                            {error.message}
                        </small>
                    ) : null}
                </div>
            );
        },
        onSettled: () =>
            Promise.all([
                queryClient.invalidateQueries(['object', objectId]),
                queryClient.invalidateQueries(['objects-owned']),
            ]),
    });

    const maxGasCoinBalance = coinBalance?.totalBalance || BigInt(0);
    const { gasBudget: gasBudgetInMist } = useGasBudgetInMist(
        DEFAULT_NFT_TRANSFER_GAS_FEE
    );
    const isInsufficientGas = maxGasCoinBalance < BigInt(gasBudgetInMist || 0);
    return (
        <Loading loading={loadingBalances}>
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
                                {isInsufficientGas ? (
                                    <div className="mt-2.5">
                                        <Alert>
                                            Insufficient balance, no individual
                                            coin found with enough balance to
                                            cover for the transfer cost
                                        </Alert>
                                    </div>
                                ) : null}
                            </Content>
                            <Menu
                                stuckClass="sendCoin-cta"
                                className="w-full px-0 pb-0 mx-0 gap-2.5"
                            >
                                <Button
                                    type="submit"
                                    variant="primary"
                                    loading={transferNFT.isLoading}
                                    disabled={
                                        !isValid ||
                                        loadingBalances ||
                                        isInsufficientGas ||
                                        !gasBudgetInMist ||
                                        loadingBalances
                                    }
                                    size="tall"
                                    text="Send NFT Now"
                                    after={<ArrowRight16 />}
                                />
                            </Menu>
                        </BottomMenuLayout>
                    </Form>
                )}
            </Formik>
        </Loading>
    );
}
