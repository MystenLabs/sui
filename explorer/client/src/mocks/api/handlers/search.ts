import { rest } from 'msw';

import { getSearchResponse } from '../data/search';

export const search = rest.get('/api/search/:term', (req, res, ctx) => {
    return res(ctx.json(getSearchResponse()));
});
