// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import express, {
    Application,
    Response as ExResponse,
    Request as ExRequest,
    NextFunction,
} from 'express';
import { ValidateError } from 'tsoa';
import morgan from 'morgan';
import bodyParser from 'body-parser';
import { RegisterRoutes } from '../build/routes';
import swaggerUi from 'swagger-ui-express';
import dotenv from 'dotenv';
import './polyfills/fetch-polyfill';

// initialize configuration
dotenv.config();

const PORT = process.env.PORT || 8000;

const app: Application = express();

app.use(morgan('tiny'));
app.use(express.static('public'));

// Set up CORS Policy
// TODO: update the allowed origin before launch
app.use(function (_, res, next) {
    res.header('Access-Control-Allow-Origin', '*');
    res.header(
        'Access-Control-Allow-Headers',
        'Origin, X-Requested-With, Content-Type, Accept'
    );
    next();
});

// Use body parser to read sent json payloads
app.use(
    bodyParser.urlencoded({
        extended: true,
    })
);
app.use(bodyParser.json());

app.use('/docs', swaggerUi.serve, async (_req: ExRequest, res: ExResponse) => {
    return res.send(
        swaggerUi.generateHTML(await import('../build/swagger.json'))
    );
});

RegisterRoutes(app);

app.use(function notFoundHandler(_req, res: ExResponse) {
    res.status(404).send({
        message: 'Not Found',
    });
});

app.use(function errorHandler(
    err: unknown,
    req: ExRequest,
    res: ExResponse,
    next: NextFunction
): ExResponse | void {
    if (err instanceof ValidateError) {
        console.warn(`Caught Validation Error for ${req.path}:`, err.fields);
        return res.status(422).json({
            message: 'Validation Failed',
            details: err?.fields,
        });
    }
    if (err instanceof Error) {
        console.warn('Internal Server Error:', err);
        return res.status(500).json({
            message: 'Internal Server Error',
        });
    }

    next();
});

app.listen(PORT, () => {
    console.log('Server is running on port', PORT);
});
