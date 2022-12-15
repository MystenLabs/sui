// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpc } from '~/hooks/useRpc';

import { useParams, useLocation } from 'react-router-dom';


function ValidatorDetails() {
    const { id } = useParams();
    const { state } = useLocation();

    
    
}

export default ValidatorDetails;
