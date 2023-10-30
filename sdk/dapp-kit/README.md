# Sui dApp Kit

The Sui dApp Kit is a set of React components, hooks, and utilities that make it easy to build a
dApp for the Sui ecosystem. It provides hooks and components for querying data from the Sui
blockchain, and connecting to Sui wallets.

See https://sui-typescript-docs.vercel.app/typescript for full documentation

### Core Features

- **Query Hooks:** dApp Kit provides a set of hooks for making rpc calls to the Sui blockchain,
  making it easy to load any information needed for your dApp.
- **Automatic Wallet State Management:** dApp Kit removes the complexity of state management related
  to wallet connections. You can focus on building your dApp.
- **Supports all Sui wallets:** No need to manually define wallets you support. All Sui wallets are
  automatically supported.
- **Easy to integrate:** dApp Kit provides pre-built React Components that you can drop right into
  your dApp, for easier integration
- **Flexible:** dApp Kit ships both fully functional React Component, and lower level hooks that you
  can use to build your own custom components.

## Install from NPM

To use the Sui dApp Kit in your project, run the following command in your project root:

```sh npm2yarn
npm i --save @mysten/dapp-kit @mysten/sui.js @tanstack/react-query
```

## Setting up Providers

To be able to use the hooks and components in the dApp Kit, you need to wrap your app with a couple
providers. The props available on the providers are covered in more detail in their respective docs
pages.

```tsx
import { createNetworkConfig, SuiClientProvider, WalletProvider } from '@mysten/dapp-kit';
import { type SuiClientOptions } from '@mysten/sui.js/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

// Config options for the networks you want to connect to
const { networkConfig } = createNetworkConfig({
	localnet: { url: getFullnodeUrl('localnet') },
	mainnet: { url: getFullnodeUrl('mainnet') },
});
const queryClient = new QueryClient();

function App() {
	return (
		<QueryClientProvider client={queryClient}>
			<SuiClientProvider networks={networkConfig} defaultNetwork="localnet">
				<WalletProvider>
					<YourApp />
				</WalletProvider>
			</SuiClientProvider>
		</QueryClientProvider>
	);
}
```

## using hooks to make rpc calls

The dApp Kit provides a set of hooks for making rpc calls to the Sui blockchain. The hooks are thin
wrappers around `useQuery` from `@tanstack/react-query`. For more comprehensive documentation on how
these query hooks checkout the
[react-query docs](https://tanstack.com/query/latest/docs/react/overview).

```tsx
import { useSuiClientQuery } from '@mysten/dapp-kit';

function MyComponent() {
	const { data, isPending, error, refetch } = useSuiClientQuery('getOwnedObjects', {
		owner: '0x123',
	});

	if (isPending) {
		return <div>Loading...</div>;
	}

	return <pre>{JSON.stringify(data, null, 2)}</pre>;
}
```
