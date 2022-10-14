// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useMemo } from 'react';
import { useParams } from 'react-router-dom';
import UserApproveContainer from '_components/user-approve-container';
import Loading from '_src/ui/app/components/loading';
import { useAppDispatch, useAppSelector, useInitializedGuard } from '_src/ui/app/hooks';
import { respondToSignatureRequest, sigRequestsSelectors } from '_src/ui/app/redux/slices/signatures';
import type { RootState } from '_redux/RootReducer';

function SigningPage() {
  const { sigId } = useParams();
  const guardLoading = useInitializedGuard(true);
  const sigRequestsLoading = useAppSelector(
    ({ signatureRequests }) => !signatureRequests.initialized
  );
  const sigRequestSelector = useMemo(
    () => (state: RootState) =>
      (sigId && sigRequestsSelectors.selectById(state, sigId)) || null,
    [sigId]
  );
  const sigRequest = useAppSelector(sigRequestSelector);
  const loading = guardLoading || sigRequestsLoading;
  const dispatch = useAppDispatch();
  const handleOnSubmit = useCallback(
    async (signed: boolean) => {
      if (sigRequest) {
        await dispatch(
          respondToSignatureRequest({
            signed,
            sigRequestId: sigRequest.id
          })
        );
      }
    },
    [dispatch, sigRequest]
  );

  useEffect(() => {
    if (
      !loading &&
      (!sigRequest || (sigRequest && sigRequest.signed !== null))
    )
      window.close();
  }, [loading, sigRequest]);

  return (
    <Loading loading={loading}>
      {sigRequest &&
        <UserApproveContainer
          origin={sigRequest.origin}
          originFavIcon={sigRequest?.originFavIcon}
          approveTitle="Sign"
          rejectTitle="Reject"
          onSubmit={handleOnSubmit}>
          <div>MESSAGE</div>
          <div>{sigRequest.message.toString()}</div>
        </UserApproveContainer>
      }
    </Loading>
  );
}

export default SigningPage;