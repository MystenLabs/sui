// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useParams } from 'react-router-dom';

import styles from './OtherDetails.module.css';

const OtherDetails = () => {
    const { term } = useParams();
    return <div className={styles.explain}>Search results for "{term}"</div>;
};

export default OtherDetails;
