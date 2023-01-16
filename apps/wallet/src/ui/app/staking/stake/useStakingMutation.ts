// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress, type SuiMoveObject } from '@mysten/sui.js';
import { useMutation } from '@tanstack/react-query';

import { useSigner, useAppSelector } from '_hooks';
import { ownedObjects } from '_redux/slices/account';
import { Coin } from '_redux/slices/sui-objects/Coin';

interface StakeTokenArgs {
    tokenTypeArg: string;
    amount: bigint;
    validatorAddress: SuiAddress;
}

export function useStakeTokenMutation() {
    const signer = useSigner();
    const allCoins = useAppSelector(ownedObjects);
    return useMutation({
        mutationFn: async ({
            tokenTypeArg,
            amount,
            validatorAddress,
        }: StakeTokenArgs) => {
            if (!validatorAddress || !amount || !tokenTypeArg) {
                throw new Error(
                    'Failed, missing required field (!principalWithdrawAmount | delegationId | stakeSuId).'
                );
            }

            const coinType = Coin.getCoinTypeFromArg(tokenTypeArg);

            const coins: SuiMoveObject[] = allCoins
                .filter(
                    (anObj) =>
                        anObj.data.dataType === 'moveObject' &&
                        anObj.data.type === coinType
                )
                .map(({ data }) => data as SuiMoveObject);

            const response = Coin.stakeCoin(
                signer,
                coins,
                amount,
                validatorAddress
            );
            return response;
        },
    });
}

interface UnStakeTokenArgs {
    principalWithdrawAmount: string;
    delegationId: string;
    stakeSuId: string;
}

export function useUnStakeTokenMutation() {
    const signer = useSigner();
    return useMutation({
        mutationFn: async ({
            principalWithdrawAmount,
            delegationId,
            stakeSuId,
        }: UnStakeTokenArgs) => {
            if (!principalWithdrawAmount || !delegationId || !stakeSuId) {
                throw new Error(
                    'Failed, missing required field (!principalWithdrawAmount | delegationId | stakeSuId).'
                );
            }

            const response = await Coin.unStakeCoin(
                signer,
                delegationId,
                stakeSuId,
                principalWithdrawAmount
            );
            return response;
        },
    });
}
