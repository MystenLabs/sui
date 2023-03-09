// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useParams } from 'react-router-dom';

function OtherDetails() {
    const { term } = useParams();
    return (
        <div className="ml-[5vw] mt-[1rem] font-sans text-2xl">
            Search results for &ldquo;{term}&rdquo;
        </div>
    );
}

export default OtherDetails;
