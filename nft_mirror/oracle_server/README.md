# Sui Oracle Service

The oracle service is a trusted entity that faciliates the cross-chain airdrop of NFT tokens on Sui. This implementation is a MVP
version that's not indended for any production use. The service currently only supports the copying of Ethereum
ERC-721 token to Sui.

As shown in the graph below, the main job of the oracle is to validate the user ownership of the NFT, call the Airdrop contract to claim the token, and return the Sui explorer link of the newly minted NFT.

![user flow](./docs/flow.png 'User Flow')

## Get Started

1. Set up the `.env` file through `cp .env.sample .env`. Rememeber to replace any placeholder value(e.g., ALCHEMY_API_KEY).
2. Install the dependencies through `npm i`
3. Start the server through `npm run dev`
4. The easiest way to test out an endpoint is through [http://localhost:8000/docs](http://localhost:8000/docs) by clicking on "Try it out". No need to manually write curl command or example data.

## Useful Commands for Development

**Requirements**: Node 14.0.0 or later version

In the project directory, you can run:

### `npm i`

Before running any of the following scripts `npm i` must run in order to install the necessary dependencies.

### `npm run dev`

Runs the app in the development mode.\
Open [http://localhost:8000](http://localhost:8000) to view it in the browser.

The page will reload if you make edits.\
You will also see any lint errors in the console.

### `npm run build`

Builds the app for production to the `build` folder.\
It bundles React in production mode and optimizes the build for the best performance.

### `npm run start`

Run the production version

### `npm run lint`

Run linting check (prettier/eslint/stylelint).

### `npm run lint:fix`

Run linting check but also try to fix any issues.

### `npm run prettier:fix:watch`

Run prettier in watch mode and format any file that changes. (Also runs prettier once in the beginning for all the files)\
It can be useful during development to format automatically all the files that change.

## Documentation

Swagger UI is available at [http://localhost:8000/docs](http://localhost:8000/docs). The UI will always show the latest documentation during development.

Aside from allowing developers to inspect the documentation, the Swagger UI also allows the developer to quickly test out the endpoint by clicking the "Try it out" button with pre-populated example data.

The syntax for documentation annotation can be found in the [tsoa docs](https://tsoa-community.github.io/docs/getting-started.html).

![swagger ui](./docs/swagger.png 'Swagger UI')
