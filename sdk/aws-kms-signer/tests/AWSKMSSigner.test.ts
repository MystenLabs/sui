// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO: write tests!

// import { AWSKMSSigner } from '../src';

// // Provided valid SECp256k1 public key
// const VALID_SECP256K1_PUBLIC_KEY = new Uint8Array([
//   2, 29, 21, 35, 7, 198, 183, 43, 14, 208, 65, 139, 14, 112, 205, 128, 231, 245, 41, 91, 141, 134,
//   245, 114, 45, 63, 82, 19, 251, 210, 57, 79, 54,
// ]);

// // Updated mock for @aws-sdk/client-kms
// vi.mock('@aws-sdk/client-kms', async () => {
//   const actual = await vi.importActual('@aws-sdk/client-kms') as any; // Use `as any` for simplicity in this example

//   // Mock only the KMSClient's send method
//   const mockSend = vi.fn().mockImplementation((command) => {
//     if (command instanceof actual.GetPublicKeyCommand) {
//       // Mock response for GetPublicKeyCommand
//       return Promise.resolve({
//         PublicKey: VALID_SECP256K1_PUBLIC_KEY,
//       });
//     } else if (command instanceof actual.SignCommand) {
//       // Mock response for SignCommand
//       return Promise.resolve({
//         Signature: Buffer.from('MockedSignature'),
//       });
//     }
//   });

//   // Return the actual exports, but with the KMSClient mocked
//   return {
//     ...actual,
//     KMSClient: vi.fn().mockImplementation(() => ({
//       send: mockSend,
//     })),
//   };
// });

// describe('AWSKMSSigner', () => {
//   let signer: AWSKMSSigner;

//   beforeAll(() => {
//     signer = new AWSKMSSigner(VALID_SECP256K1_PUBLIC_KEY);
//   });

//   it('should create an instance successfully', () => {
//     expect(signer).toBeInstanceOf(AWSKMSSigner);
//   });

//   it('should return the correct key scheme', async () => {
//     const scheme = signer.getKeyScheme();
//     expect(scheme).toEqual('Secp256k1');
//   });

//     it('should set and get a public key', async () => {
//         signer.setPublicKey(VALID_SECP256K1_PUBLIC_KEY);
//         // Assuming getPublicKey() returns a Secp256k1PublicKey instance
//         // that can be directly compared to VALID_SECP256K1_PUBLIC_KEY for equality
//         const publicKey = signer.getPublicKey().toSuiBytes();
//         console.log(publicKey);
//         expect(publicKey).toEqual(VALID_SECP256K1_PUBLIC_KEY);
//     });

//   // Add more tests as needed to cover the functionality of your class
// });
