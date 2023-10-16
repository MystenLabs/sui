// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { expect, test } from 'vitest';

import { jwtToAddress } from '../src/address.js';

test('a valid JWT should not throw an error', () => {
	const jwt =
		'eyJraWQiOiJzdWkta2V5LWlkIiwidHlwIjoiSldUIiwiYWxnIjoiUlMyNTYifQ.eyJzdWIiOiI4YzJkN2Q2Ni04N2FmLTQxZmEtYjZmYy02M2U4YmI3MWZhYjQiLCJhdWQiOiJ0ZXN0IiwibmJmIjoxNjk3NDY1NDQ1LCJpc3MiOiJodHRwczovL29hdXRoLnN1aS5pbyIsImV4cCI6MTY5NzU1MTg0NSwibm9uY2UiOiJoVFBwZ0Y3WEFLYlczN3JFVVM2cEVWWnFtb0kifQ.P8-BpCKsoCi93if1UdAnzBtVxrFgVUC-k3ZwYYbXO2_FNW58VLmxAiOmFB1g17Qph5N5D8cPF2j6ANnBl9xqH9_dOF9zGhjkR_6il28jj4rSFP5I8mBJ_iE1xwK6VKu2HtJkt1t94FEspwC78sd9HbWBbXcbmp7ivEo6ZOzfqCr5bggV5YeAxxoLC3cdRuVoNsdCOnMku9aMtRn9F7E_Dd8WCFO5ewjm1Rm2ZDIWose7ohhug_eCyHEVYPq6VsEBOj_zop0yShT8WDyXb2dAHk3YublxvSx2sgakTs_WdXpAXXzewPF5rnX60qQB21Xbqm97dAWvPg9384ItkI5xMA';
	const userSalt = '248191903847969014646285995941615069143';
	const address = jwtToAddress(jwt, userSalt);
	expect(address).toBe('0x22cebcf68a9d75d508d50d553dd6bae378ef51177a3a6325b749e57e3ba237d6');
});
