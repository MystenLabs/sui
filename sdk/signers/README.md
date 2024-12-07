# Sui KMS Signers

The Sui KMS Signers package provides a set of tools for securely signing transactions using Key
Management Services (KMS) like AWS KMS.

## Table of Contents

- [AWS KMS Signer](#aws-kms-signer)
  - [Usage](#usage)
  - [API](#api)
    - [fromKeyId](#fromkeyid)
      - [Parameters](#parameters)
      - [Examples](#examples)

## AWS KMS Signer

The AWS KMS Signer allows you to leverage AWS's Key Management Service to sign Sui transactions.

### Usage

```typescript
import { AwsKmsSigner } from '@mysten/signers/aws';

const prepareSigner = async () => {
	const { AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_REGION, AWS_KMS_KEY_ID } = process.env;

	return AwsKmsSigner.fromKeyId(AWS_KMS_KEY_ID, {
		region: AWS_REGION,
		accessKeyId: AWS_ACCESS_KEY_ID,
		secretAccessKey: AWS_SECRET_ACCESS_KEY,
	});
};
```

### API

#### fromKeyId

Create an AWS KMS signer from AWS Key ID and AWS credentials. This method initializes the signer
with the necessary AWS credentials and region information, allowing it to interact with AWS KMS to
perform cryptographic operations.

##### Parameters

- `keyId`
  **[string](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/String)**
  The AWS KMS key ID.
- `options`
  **[object](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Object)** An
  object containing AWS credentials and region.
  - `region`
    **[string](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/String)**
    The AWS region.
  - `accessKeyId`
    **[string](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/String)**
    The AWS access key ID.
  - `secretAccessKey`
    **[string](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/String)**
    The AWS secret access key.

##### Examples

```typescript
const signer = await AwsKmsSigner.fromKeyId('your-kms-key-id', {
	region: 'us-west-2',
	accessKeyId: 'your-access-key-id',
	secretAccessKey: 'your-secret-access-key',
});
```

Returns
**[Promise](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Promise)&lt;[AwsKmsSigner](./src/aws/aws-kms-signer.ts)>**
An instance of AwsKmsSigner.

**Notice**: AWS Signer requires Node >=20 due to dependency on `crypto`
