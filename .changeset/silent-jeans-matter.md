---
'@mysten/sui': minor
'@mysten/zklogin': minor
---

All functionality from `@mysten/zklogin` has been moved to `@mysten/sui/zklogin`

For most methods, simply replace the `@mysten/zklogin` import with `@mysten/sui/zklogin`

2 Methods require one small additional change:

`computeZkLoginAddress` and `jwtToAddress` have new `legacyAddress` flags which must be set to true for backwards compatibility:

```diff
- import { computeZkLoginAddress, jwtToAddress } from '@mysten/zklogin';
+ import { computeZkLoginAddress, jwtToAddress } from '@mysten/sui/zklogin';

  const address = jwtToAddress(
   jwtAsString,
   salt,
+  true
  );
  const address = computeZkLoginAddress({
	claimName,
	claimValue,
	iss,
	aud,
	userSalt: BigInt(salt),
+	legacyAddress: true,
  });
```
