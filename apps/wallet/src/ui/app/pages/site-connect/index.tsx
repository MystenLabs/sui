// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { formatAddress, type SuiAddress } from '@mysten/sui.js';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useParams } from 'react-router-dom';

import { DAppPermissionsList } from '../../components/DAppPermissionsList';
import { SummaryCard } from '../../components/SummaryCard';
import { WalletListSelect } from '../../components/WalletListSelect';
import { Text } from '../../shared/text';
import Loading from '_components/loading';
import UserApproveContainer from '_components/user-approve-container';
import { useAppDispatch, useAppSelector } from '_hooks';
import {
    permissionsSelectors,
    respondToPermissionRequest,
} from '_redux/slices/permissions';
import { FEATURES } from '_src/shared/experimentation/features';

import type { RootState } from '_redux/RootReducer';

import st from './SiteConnectPage.module.scss';

function SiteConnectPage() {
    const { requestID } = useParams();
    const permissionsInitialized = useAppSelector(
        ({ permissions }) => permissions.initialized
    );
    const loading = !permissionsInitialized;
    const permissionSelector = useMemo(
        () => (state: RootState) =>
            requestID
                ? permissionsSelectors.selectById(state, requestID)
                : null,
        [requestID]
    );
    const dispatch = useAppDispatch();
    const permissionRequest = useAppSelector(permissionSelector);
    const activeAccount = useAppSelector(({ account }) => account.address);
    const isMultiAccountEnabled = useFeature(FEATURES.WALLET_MULTI_ACCOUNTS).on;
    const [accountsToConnect, setAccountsToConnect] = useState<SuiAddress[]>(
        () => (activeAccount ? [activeAccount] : [])
    );
    const handleOnSubmit = useCallback(
        (allowed: boolean) => {
            if (requestID && accountsToConnect) {
                dispatch(
                    respondToPermissionRequest({
                        id: requestID,
                        accounts: allowed ? accountsToConnect : [],
                        allowed,
                    })
                );
            }
        },
        [dispatch, requestID, accountsToConnect]
    );
    useEffect(() => {
        if (
            !loading &&
            (!permissionRequest || permissionRequest.responseDate)
        ) {
            window.close();
        }
    }, [loading, permissionRequest]);

    const parsedOrigin = useMemo(
        () => (permissionRequest ? new URL(permissionRequest.origin) : null),
        [permissionRequest]
    );

    const isSecure = parsedOrigin?.protocol === 'https:';
    const [displayWarning, setDisplayWarning] = useState(!isSecure);

    const handleHideWarning = useCallback(
        (allowed: boolean) => {
            if (allowed) {
                setDisplayWarning(false);
            } else {
                handleOnSubmit(false);
            }
        },
        [handleOnSubmit]
    );

    useEffect(() => {
        setDisplayWarning(!isSecure);
    }, [isSecure]);

    return (
        <Loading loading={loading}>
            {permissionRequest &&
                (displayWarning ? (
                    <UserApproveContainer
                        origin={permissionRequest.origin}
                        originFavIcon={permissionRequest.favIcon}
                        approveTitle="Continue"
                        rejectTitle="Reject"
                        onSubmit={handleHideWarning}
                        isWarning
                        isConnect
                        addressHidden
                    >
                        <div className={st.warningWrapper}>
                            <h1 className={st.warningTitle}>
                                Your Connection is Not Secure
                            </h1>
                        </div>

                        <div className={st.warningMessage}>
                            This site requesting this wallet connection is not
                            secure, and attackers might be trying to steal your
                            information.
                            <br />
                            <br />
                            Continue at your own risk.
                        </div>
                    </UserApproveContainer>
                ) : (
                    <UserApproveContainer
                        origin={permissionRequest.origin}
                        originFavIcon={permissionRequest.favIcon}
                        approveTitle="Connect"
                        rejectTitle="Reject"
                        onSubmit={handleOnSubmit}
                        isConnect
                        addressHidden
                        approveDisabled={!accountsToConnect.length}
                    >
                        <SummaryCard
                            header="Permissions requested"
                            body={
                                <DAppPermissionsList
                                    permissions={permissionRequest.permissions}
                                />
                            }
                        />
                        {isMultiAccountEnabled ? (
                            <WalletListSelect
                                title="Connect Accounts"
                                values={accountsToConnect}
                                onChange={setAccountsToConnect}
                            />
                        ) : (
                            <SummaryCard
                                header="Connect To Account"
                                body={
                                    <Text
                                        mono
                                        color="steel-dark"
                                        variant="body"
                                        weight="semibold"
                                    >
                                        {activeAccount
                                            ? formatAddress(activeAccount)
                                            : null}
                                    </Text>
                                }
                            />
                        )}
                    </UserApproveContainer>
                ))}
        </Loading>
    );
}

export default SiteConnectPage;
