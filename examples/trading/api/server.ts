// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cors from 'cors';
import express from 'express';

import { prisma } from './db';
import {
	formatPaginatedResponse,
	parsePaginationForQuery,
	parseWhereStatement,
	WhereParam,
	WhereParamTypes,
} from './utils/api-queries';

const app = express();
app.use(cors());

app.use(express.json());

app.get('/', async (req, res) => {
	return res.send({ message: '🚀 API is functional 🚀' });
});

app.get('/locked', async (req, res) => {
	const acceptedQueries: WhereParam[] = [
		{
			key: 'deleted',
			type: WhereParamTypes.BOOLEAN,
		},
		{
			key: 'creator',
			type: WhereParamTypes.STRING,
		},
		{
			key: 'keyId',
			type: WhereParamTypes.STRING,
		},
		{
			key: 'objectId',
			type: WhereParamTypes.STRING,
		},
	];

	try {
		const locked = await prisma.locked.findMany({
			where: parseWhereStatement(req.query, acceptedQueries)!,
			...parsePaginationForQuery(req.query),
		});

		return res.send(formatPaginatedResponse(locked));
	} catch (e) {
		console.error(e);
		return res.status(400).send(e);
	}
});

app.get('/escrows', async (req, res) => {
	const acceptedQueries: WhereParam[] = [
		{
			key: 'cancelled',
			type: WhereParamTypes.BOOLEAN,
		},
		{
			key: 'swapped',
			type: WhereParamTypes.BOOLEAN,
		},
		{
			key: 'recipient',
			type: WhereParamTypes.STRING,
		},
		{
			key: 'sender',
			type: WhereParamTypes.STRING,
		},
	];

	try {
		const escrows = await prisma.escrow.findMany({
			where: parseWhereStatement(req.query, acceptedQueries)!,
			...parsePaginationForQuery(req.query),
		});

		return res.send(formatPaginatedResponse(escrows));
	} catch (e) {
		console.error(e);
		return res.status(400).send(e);
	}
});

app.listen(3000, () => console.log(`🚀 Server ready at: http://localhost:3000`));
