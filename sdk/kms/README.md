# Sui KMS Signers

This package is the source for finding exported KMS signers.

## AWS KMS Signer

You can use AWS KMS signer like the following:

```typescript
import { AwsKmsSigner } from '@mysten/kms/aws';

const prepareSigner = async () => {
	const { AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_REGION, AWS_KMS_KEY_ID } = process.env;

	return AwsKmsSigner.fromCredentials(AWS_KMS_KEY_ID, {
		region: AWS_REGION,
		accessKeyId: AWS_ACCESS_KEY_ID,
		secretAccessKey: AWS_SECRET_ACCESS_KEY,
	});
};
```
