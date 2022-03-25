# Sui Explorer Client

## Development

**Requirements**: Node 14.0.0 or later version

In the project directory, run:

### `npm i`

Before running any of the following scripts `npm i` must run in order to install the necessary dependencies.

### `npm start`

Runs the app in the development mode.

Open http://localhost:3000 to view it in the browser.

The page will reload if you make edits. You will also see any lint errors in the console.

### `npm run start:dev`

Same as `npm start` but runs `prettier:fix:watch` to format the files.

### `npm test`

Launches the test runner in the interactive watch mode.

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
