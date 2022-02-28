import faker from '@faker-js/faker';
import { rest } from 'msw';

import { getTransaction } from '../data/transactions';

export const transactions = rest.get('/api/transactions', (req, res, ctx) => {
    return res(
        ctx.json(
            Array.from({ length: faker.datatype.number(15) }, getTransaction)
        )
    );
});

export const transaction = rest.get(
    '/api/transactions/:id',
    (req, res, ctx) => {
        const id = Array.isArray(req.params.id)
            ? req.params.id?.[0]
            : req.params.id;
        return res(ctx.json(getTransaction(id)));
    }
);
