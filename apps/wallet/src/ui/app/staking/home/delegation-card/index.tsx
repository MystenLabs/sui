// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Delegation } from '@mysten/sui.js';
import cl from 'classnames';
import { memo, useMemo } from 'react';

import CoinBalance from '_app/shared/coin-balance';
import { epochSelector, getValidatorSelector } from '_app/staking/selectors';
import Icon, { SuiIcons } from '_components/icon';
import { useAppSelector } from '_hooks';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';
import { suiObjectsAdapterSelectors } from '_redux/slices/sui-objects/index';

import type { RootState } from '_redux/RootReducer';

import st from './DelegationCard.module.scss';

export type DelegationCardProps = {
    className?: string;
    id: string;
};

function DelegationCard({ className, id }: DelegationCardProps) {
    const delegationSelector = useMemo(
        () => (state: RootState) => {
            const suiObj = suiObjectsAdapterSelectors.selectById(state, id);
            if (suiObj && Delegation.isDelegationSuiObject(suiObj)) {
                return new Delegation(suiObj);
            }
            return undefined;
        },
        [id]
    );
    const delegation = useAppSelector(delegationSelector);
    const epoch = useAppSelector(epochSelector);
    const validatorAddress = delegation?.validatorAddress();
    const validatorSelector = useMemo(
        () => getValidatorSelector(validatorAddress),
        [validatorAddress]
    );
    const validator = useAppSelector(validatorSelector);
    const validatorName = useMemo(() => {
        if (validator) {
            return Buffer.from(validator.fields.name, 'base64').toString();
        }
        return null;
    }, [validator]);
    return (
        <div className={cl(st.container, className)}>
            <div className={st.iconRow}>
                <Icon icon="columns-gap" />
            </div>
            <div className={st.validator}>
                {validatorName || validatorAddress}
            </div>
            {/* TODO: show the APY of the validator. How can we get it? */}
            <div className={st.apy}>? APY</div>
            <div className={st.balance}>
                <CoinBalance
                    balance={BigInt(delegation?.delegateAmount?.() || 0)}
                    type={GAS_TYPE_ARG}
                    className={st.balance}
                />
            </div>
            {epoch !== null && delegation?.hasUnclaimedRewards(epoch) ? (
                <Icon icon="circle-fill" className={st.rewards} />
            ) : null}
            <Icon icon={SuiIcons.ArrowRight} className={st.arrow} />
        </div>
    );
}

export default memo(DelegationCard);
