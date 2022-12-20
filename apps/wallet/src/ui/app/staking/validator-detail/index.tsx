// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiObject, isSuiMoveObject } from '@mysten/sui.js';
import { useState, useMemo } from 'react';
import { useSearchParams, useNavigate, Navigate } from 'react-router-dom';

import { getName, STATE_OBJECT } from '../usePendingDelegation';
import { ValidatorDetailCard } from './ValidatorDetailCard';
import { ImageIcon } from '_app/shared/image-icon';
import Alert from '_components/alert';
import { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { useAppSelector, useObjectsState, useGetObject } from '_hooks';

import type { ValidatorState } from '../ValidatorDataTypes';

export function ValidatorDetail() {
    const { loading, error, showError } = useObjectsState();
    const [searchParams] = useSearchParams();
    const validatorAddressParams = searchParams.get('address');
    const [showModal, setShowModal] = useState(true);

    const accountAddress = useAppSelector(({ account }) => account.address);
    const { data, isLoading } = useGetObject(STATE_OBJECT);

    const navigate = useNavigate();
    const close = () => {
        navigate('/');
    };

    const validatorsData =
        data && isSuiObject(data.details) && isSuiMoveObject(data.details.data)
            ? (data.details.data.fields as ValidatorState)
            : null;

    const validatorData = useMemo(() => {
        if (!validatorsData) return null;

        const validator =
            validatorsData.validators.fields.active_validators.find(
                (av) =>
                    av.fields.metadata.fields.sui_address ===
                    validatorAddressParams
            );

        if (!validator) return null;

        const {
            sui_balance,
            starting_epoch,
            pending_delegations,
            delegation_token_supply,
        } = validator.fields.delegation_staking_pool.fields;

        const num_epochs_participated = validatorsData.epoch - starting_epoch;
        const { name: rawName, sui_address } = validator.fields.metadata.fields;

        const APY = Math.pow(
            1 +
                (sui_balance - delegation_token_supply.fields.value) /
                    delegation_token_supply.fields.value,
            365 / num_epochs_participated - 1
        );
        const pending_delegationsByAddress = pending_delegations
            ? pending_delegations.filter(
                  (d) => d.fields.delegator === accountAddress
              )
            : [];

        return {
            name: getName(rawName),
            commissionRate: validator.fields.commission_rate,
            apy: APY > 0 ? APY : 'N/A',
            logo: null,
            address: sui_address,
            totalStaked: pending_delegations.reduce(
                (acc, fields) =>
                    (acc += BigInt(fields.fields.sui_amount || 0n)),
                0n
            ),
            // TODO: Calculate suiEarned
            suiEarned: 0n,
            pendingDelegationAmount: pending_delegationsByAddress.reduce(
                (acc, fields) =>
                    (acc += BigInt(fields.fields.sui_amount || 0n)),
                0n
            ),
        };
    }, [accountAddress, validatorAddressParams, validatorsData]);

    if (!validatorAddressParams) {
        return <Navigate to={'/stake'} replace={true} />;
    }

    const validatorName = validatorData?.name || 'Loading...';

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title={
                <div className="flex gap-2 items-center capitalize">
                    <ImageIcon
                        src={validatorData?.logo}
                        alt={validatorName}
                        size="small"
                    />
                    {validatorName}
                </div>
            }
            closeIcon={SuiIcons.Close}
            closeOverlay={close}
        >
            <Loading
                className="w-full flex justify-center items-center"
                loading={loading || isLoading}
            >
                {showError && error && (
                    <Alert className="mb-2">
                        <strong>Sync error (data might be outdated).</strong>
                        <small>{error.message}</small>
                    </Alert>
                )}

                {validatorData && (
                    <ValidatorDetailCard
                        validatorAddress={validatorData.address}
                        pendingDelegationAmount={
                            validatorData.pendingDelegationAmount || 0n
                        }
                        suiEarned={validatorData.suiEarned}
                        apy={validatorData.apy}
                        commissionRate={validatorData.commissionRate}
                    />
                )}
            </Loading>
        </Overlay>
    );
}
