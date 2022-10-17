// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useMemo, useState } from 'react';
import { useParams } from 'react-router-dom';

import UserApproveContainer from '_components/user-approve-container';
import Loading from '_src/ui/app/components/loading';
import { useAppDispatch, useAppSelector } from '_src/ui/app/hooks';
import { respondToSignatureRequest, sigRequestsSelectors } from '_src/ui/app/redux/slices/signatures';

import type { RootState } from '_redux/RootReducer';

import st from './SigningPage.module.scss';

function SigningPage() {
  const { sigId } = useParams();
  const [message, setMessage] = useState<string>("<div></div>");
  const sigRequestsLoading = useAppSelector(
    ({ signatureRequests }) => !signatureRequests.initialized
  );
  const sigRequestSelector = useMemo(
    () => (state: RootState) =>
      (sigId && sigRequestsSelectors.selectById(state, sigId)) || null,
    [sigId]
  );
  const sigRequest = useAppSelector(sigRequestSelector);
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
      !sigRequestsLoading &&
      (!sigRequest || (sigRequest && sigRequest.signed !== null))
    )
      window.close();
  }, [sigRequestsLoading, sigRequest]);

  useEffect(() => {
    if (sigRequest) {
      const data = [];
      for (let i = 0; i < Object.keys(sigRequest.message).length; i++)
        data.push(sigRequest.message[i]);
      setMessage(`<div>${(new TextDecoder().decode(Uint8Array.from(data))).replace(/\n/g, "<br/>")}</div>`);
    }
  }, [sigRequest]);

  return (
    <Loading loading={sigRequestsLoading}>
      {sigRequest &&
        <UserApproveContainer
          origin={sigRequest.origin}
          originFavIcon={sigRequest?.originFavIcon}
          approveTitle="Sign"
          rejectTitle="Reject"
          onSubmit={handleOnSubmit}>
          <div className={st.warningWrapper}>
            <h1 className={st.warningTitle}>Message</h1>
          </div>
          <div className={st.warningMessage} dangerouslySetInnerHTML={{ __html: message }}></div>
        </UserApproveContainer>
      }
    </Loading>
  );
}

export default SigningPage;