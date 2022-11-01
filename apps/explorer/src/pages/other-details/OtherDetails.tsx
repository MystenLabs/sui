// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useParams } from 'react-router-dom';

import styles from './OtherDetails.module.css';

function OtherDetails() {
    const { term } = useParams();
    return (
        <div className={styles.explain}>
            Search results for &ldquo;{term}&rdquo;
        </div>
    );
}

export default OtherDetails;
