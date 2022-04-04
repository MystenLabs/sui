# SuiExplorer Client

# Set Up

**Requirements**: Node 14.0.0 or later version

In the project directory, run:

### `npm i`

Before running any of the following scripts `npm i` must run in order to install the necessary dependencies.

# How to Switch Environment

The purpose of the SuiExplorer Client is to present data extracted from a real or theoretical Sui Network.

What the 'Sui Network' is varies according to the environment variable `REACT_APP_DATA`.

When running most of the below npm commands, the SuiExplorer Client will extract and present data from the Sui Network connected to the URL https://demo-rpc.sui.io.

If the environment variable `REACT_APP_DATA` is set to `static`, then the SuiExplorer will instead pull data from a local, static JSON dataset that can be found at `./src/utils/static/mock_data.json`.

For example, suppose we wish to locally run the website using the static JSON dataset and not the API, then we would run the following:

```bash
REACT_APP_DATA=static npm start
```

Note that the commands `npm run test` and `npm run start:static` are the exceptions. Here the SuiExplorer will instead use the static JSON dataset. The tests have been written to specifically check the UI and not the API connection and so use the static JSON dataset.

## NPM Commands and what they do

### `npm start`

Runs the app as connected to the API at https://demo-rpc.sui.io.

Open http://localhost:3000 to view it in the browser.

The page will reload if you make edits. You will also see any lint errors in the console.

### `npm run start:static`

Runs the app as connected to the static JSON dataset with `REACT_APP_DATA` set to `static`.

Open http://localhost:8080 to view it in the browser.

The page will reload when edits are made. You can run `npm start` and `npm run start:static` at the same time because they use different ports.

### `npm run start:dev`

Same as `npm start` but runs `prettier:fix:watch` to format the files.

### `npm test`

This runs a series of end-to-end, browser tests usng the website as connected to the static JSON dataset. This command is run by the GitHub checks. The tests must pass before merging a branch into main.

### `npm run build`

Builds the app for production to the `build` folder.

It bundles React in production mode and optimizes the build for the best performance.

### `npm run lint`

Run linting check (prettier/eslint/stylelint).

### `npm run lint:fix`

Run linting check but also try to fix any issues.

### `npm run prettier:fix:watch`

Run prettier in watch mode and format any file that changes. (Also runs prettier once in the beginning for all the files)\
It can be useful during development to format automatically all the files that change.

## Deployment

For guidance on deployment, plese see here: https://create-react-app.dev/docs/deployment/

Because of the addition of `react-router`, further changes will be needed that depend on the exact infrastructure used. Please consult section **Serving Apps with Client-Side Routing**.
