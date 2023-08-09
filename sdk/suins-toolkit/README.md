# SuiNS TypeScript SDK

This is a lightweight SDK (1kB minified bundle size), providing utility classes and functions for applications to interact with on-chain `.sui` names registered from [Sui Name Service (suins.io)](https://suins.io).

## Getting started

The SDK is published to [npm registry](https://www.npmjs.com/package/@mysten/suins-toolkit). To use it in your project:

```bash
$ npm install @mysten/suins-toolkit
```

You can also use yarn or pnpm.

## Examples

Create an instance of SuinsClient:

```typescript
import { SuiClient } from '@mysten/sui.js/client';
import { SuinsClient } from '@mysten/suins-toolkit';

const client = new SuiClient();
export const suinsClient = new SuinsClient(client);
```

Choose network type:

```typescript
export const suinsClient = new SuinsClient(client, {
    networkType: 'testnet',
});
```

> **Note:** To ensure best performance, please make sure to create only one instance of the SuinsClient class in your application. Then, import the created `suinsClient` instance to use its functions.

Fetch an address linked to a name:

```typescript
const address = await suinsClient.getAddress('suins.sui');
```

Fetch the default name of an address:

```typescript
const defaultName = await suinsClient.getName(
    '0xc2f08b6490b87610629673e76bab7e821fe8589c7ea6e752ea5dac2a4d371b41',
);
```

Fetch a name object:

```typescript
const nameObject = await suinsClient.getNameObject('suins.sui');
```

Fetch a name object including the owner:

```typescript
const nameObject = await suinsClient.getNameObject('suins.sui', {
    showOwner: true,
});
```

Fetch a name object including the Avatar the owner has set (it automatically includes owner too):

```typescript
const nameObject = await suinsClient.getNameObject('suins.sui', {
    showOwner: true, // this can be skipped as showAvatar includes it by default
    showAvatar: true,
});
```

## License

[Apache-2.0](https://github.com/SuiNSdapp/toolkit/blob/main/LICENSE)
