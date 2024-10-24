// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::PartialVMResult;
use move_core_types::gas_algebra::InternalGas;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{Value, VectorRef},
};
use smallvec::smallvec;
use std::collections::VecDeque;
// todo: remove this dep
use aws_nitro_enclaves_cose::crypto::{Hash, MessageDigest};
use aws_nitro_enclaves_cose::error::CoseError;
use openssl::x509::X509;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub fn aws_attestation_verify(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    // todo: figure out cost

    let attestation = pop_arg!(args, VectorRef);
    let user_data = pop_arg!(args, VectorRef);

    let attestation_ref = attestation.as_bytes_ref();
    let user_data_ref = user_data.as_bytes_ref();

    let pem_data = r"-----BEGIN CERTIFICATE-----
MIICETCCAZagAwIBAgIRAPkxdWgbkK/hHUbMtOTn+FYwCgYIKoZIzj0EAwMwSTEL
MAkGA1UEBhMCVVMxDzANBgNVBAoMBkFtYXpvbjEMMAoGA1UECwwDQVdTMRswGQYD
VQQDDBJhd3Mubml0cm8tZW5jbGF2ZXMwHhcNMTkxMDI4MTMyODA1WhcNNDkxMDI4
MTQyODA1WjBJMQswCQYDVQQGEwJVUzEPMA0GA1UECgwGQW1hem9uMQwwCgYDVQQL
DANBV1MxGzAZBgNVBAMMEmF3cy5uaXRyby1lbmNsYXZlczB2MBAGByqGSM49AgEG
BSuBBAAiA2IABPwCVOumCMHzaHDimtqQvkY4MpJzbolL//Zy2YlES1BR5TSksfbb
48C8WBoyt7F2Bw7eEtaaP+ohG2bnUs990d0JX28TcPQXCEPZ3BABIeTPYwEoCWZE
h8l5YoQwTcU/9KNCMEAwDwYDVR0TAQH/BAUwAwEB/zAdBgNVHQ4EFgQUkCW1DdkF
R+eWw5b6cp3PmanfS5YwDgYDVR0PAQH/BAQDAgGGMAoGCCqGSM49BAMDA2kAMGYC
MQCjfy+Rocm9Xue4YnwWmNJVA44fA0P5W2OpYow9OYCVRaEevL8uO1XYru5xtMPW
rfMCMQCi85sWBbJwKKXdS6BptQFuZbT73o/gBh1qUxl/nNr12UO8Yfwr6wPLb+6N
IwLz3/Y=
-----END CERTIFICATE-----";
    let x509 = X509::from_pem(pem_data.as_bytes()).unwrap();
    let trusted_root_cert = x509.to_der().unwrap();

    let (_protected, payload, _signature) = AttestationDocument::parse(&attestation_ref)
        .map_err(|err| format!("AttestationDocument::authenticate parse failed:{:?}", err))
        .unwrap();

    let document = AttestationDocument::parse_payload(&payload)
        .map_err(|err| format!("AttestationDocument::authenticate failed:{:?}", err))
        .unwrap();
    if document.clone().user_data.unwrap() != user_data_ref.to_vec() {
        return Ok(NativeResult::ok(
            InternalGas::zero(),
            smallvec![Value::bool(false)],
        ));
    }
    if AttestationDocument::authenticate(&document, &attestation_ref, &trusted_root_cert).is_err() {
        Ok(NativeResult::ok(
            InternalGas::zero(),
            smallvec![Value::bool(false)],
        ))
    } else {
        Ok(NativeResult::ok(
            InternalGas::zero(),
            smallvec![Value::bool(true)],
        ))
    }
}

/// Type that implements various cryptographic traits using the OpenSSL library
pub struct Openssl;

impl Hash for Openssl {
    fn hash(digest: MessageDigest, data: &[u8]) -> Result<Vec<u8>, CoseError> {
        openssl::hash::hash(digest.into(), data)
            .map_err(|e| CoseError::HashingError(Box::new(e)))
            .map(|h| h.to_vec())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AttestationDocument {
    pub module_id: String,
    pub timestamp: u64,
    pub digest: String,
    pub pcrs: Vec<Vec<u8>>,
    pub certificate: Vec<u8>,
    pub cabundle: Vec<Vec<u8>>,
    pub public_key: Option<Vec<u8>>,
    pub user_data: Option<Vec<u8>>,
    pub nonce: Option<Vec<u8>>,
}

impl AttestationDocument {
    pub fn authenticate(
        document: &AttestationDocument,
        document_data: &[u8],
        trusted_root_cert: &[u8],
    ) -> Result<(), String> {
        // Following the steps here: https://docs.aws.amazon.com/enclaves/latest/user/verify-root.html
        // Step 1. Decode the CBOR object and map it to a COSE_Sign1 structure
        // Step 2. Exract the attestation document from the COSE_Sign1 structure
        // Step 3. Verify the certificate's chain
        let mut certs: Vec<rustls::Certificate> = Vec::new();
        for this_cert in document.cabundle.clone().iter().rev() {
            let cert = rustls::Certificate(this_cert.to_vec());
            certs.push(cert);
        }
        let cert = rustls::Certificate(document.certificate.clone());
        certs.push(cert);

        let mut root_store = rustls::RootCertStore::empty();
        root_store
            .add(&rustls::Certificate(trusted_root_cert.to_vec()))
            .map_err(|err| {
                format!(
                    "AttestationDocument::authenticate failed to add trusted root cert:{:?}",
                    err
                )
            })?;

        let verifier = rustls::server::AllowAnyAuthenticatedClient::new(root_store);
        let _verified = verifier
            .verify_client_cert(
                &rustls::Certificate(document.certificate.clone()),
                &certs,
                std::time::SystemTime::now(),
            )
            .map_err(|err| {
                format!(
                    "AttestationDocument::authenticate verify_client_cert failed:{:?}",
                    err
                )
            })?;
        // if verify_client_cert didn't generate an error, authentication passed

        // Step 4. Ensure the attestation document is properly signed
        let authenticated = {
            let sig_structure = aws_nitro_enclaves_cose::sign::CoseSign1::from_bytes(document_data)
                .map_err(|err| {
                    format!("AttestationDocument::authenticate failed to load document_data as COSESign1 structure:{:?}", err)
                })?;
            let cert = openssl::x509::X509::from_der(&document.certificate)
                .map_err(|err| {
                    format!("AttestationDocument::authenticate failed to parse document.certificate as X509 certificate:{:?}", err)
                })?;
            let public_key = cert.public_key()
                .map_err(|err| {
                    format!("AttestationDocument::authenticate failed to extract public key from certificate:{:?}", err)
                })?;

            sig_structure.verify_signature::<Openssl>(&public_key)
                .map_err(|err| {
                    format!("AttestationDocument::authenticate failed to verify signature on sig_structure:{:?}", err)
                })?
        };
        if !authenticated {
            Err(format!(
                "AttestationDocument::authenticate invalid COSE certificate for provided key"
            ))
        } else {
            Ok(())
        }
    }

    pub fn parse(document_data: &[u8]) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>), String> {
        let cbor: serde_cbor::Value = serde_cbor::from_slice(document_data)
            .map_err(|err| format!("AttestationDocument::parse from_slice failed:{:?}", err))?;
        let elements = match cbor {
            serde_cbor::Value::Array(elements) => elements,
            _ => panic!("AttestationDocument::parse Unknown field cbor:{:?}", cbor),
        };
        let protected = match &elements[0] {
            serde_cbor::Value::Bytes(prot) => prot,
            _ => panic!(
                "AttestationDocument::parse Unknown field protected:{:?}",
                elements[0]
            ),
        };
        let _unprotected = match &elements[1] {
            serde_cbor::Value::Map(unprot) => unprot,
            _ => panic!(
                "AttestationDocument::parse Unknown field unprotected:{:?}",
                elements[1]
            ),
        };
        let payload = match &elements[2] {
            serde_cbor::Value::Bytes(payld) => payld,
            _ => panic!(
                "AttestationDocument::parse Unknown field payload:{:?}",
                elements[2]
            ),
        };
        let signature = match &elements[3] {
            serde_cbor::Value::Bytes(sig) => sig,
            _ => panic!(
                "AttestationDocument::parse Unknown field signature:{:?}",
                elements[3]
            ),
        };
        Ok((protected.to_vec(), payload.to_vec(), signature.to_vec()))
    }

    pub fn parse_payload(payload: &Vec<u8>) -> Result<AttestationDocument, String> {
        let document_data: serde_cbor::Value = serde_cbor::from_slice(payload.as_slice())
            .map_err(|err| format!("document parse failed:{:?}", err))?;

        let document_map: BTreeMap<serde_cbor::Value, serde_cbor::Value> = match document_data {
            serde_cbor::Value::Map(map) => map,
            _ => {
                return Err(format!(
                    "AttestationDocument::parse_payload field ain't what it should be:{:?}",
                    document_data
                ))
            }
        };

        let module_id: String =
            match document_map.get(&serde_cbor::Value::Text("module_id".to_string())) {
                Some(serde_cbor::Value::Text(val)) => val.to_string(),
                _ => {
                    return Err(
                        "AttestationDocument::parse_payload module_id is wrong type or not present"
                            .to_string(),
                    )
                }
            };

        let timestamp: i128 =
            match document_map.get(&serde_cbor::Value::Text("timestamp".to_string())) {
                Some(serde_cbor::Value::Integer(val)) => *val,
                _ => {
                    return Err(
                        "AttestationDocument::parse_payload timestamp is wrong type or not present"
                            .to_string(),
                    )
                }
            };

        let timestamp: u64 = timestamp.try_into().map_err(|err| {
            format!(
                "AttestationDocument::parse_payload failed to convert timestamp to u64:{:?}",
                err
            )
        })?;

        let public_key: Option<Vec<u8>> =
            match document_map.get(&serde_cbor::Value::Text("public_key".to_string())) {
                Some(serde_cbor::Value::Bytes(val)) => Some(val.to_vec()),
                Some(_null) => None,
                None => None,
            };

        let certificate: Vec<u8> =
            match document_map.get(&serde_cbor::Value::Text("certificate".to_string())) {
                Some(serde_cbor::Value::Bytes(val)) => val.to_vec(),
                _ => return Err(
                    "AttestationDocument::parse_payload certificate is wrong type or not present"
                        .to_string(),
                ),
            };

        let pcrs: Vec<Vec<u8>> = match document_map
            .get(&serde_cbor::Value::Text("pcrs".to_string()))
        {
            Some(serde_cbor::Value::Map(map)) => {
                let mut ret_vec: Vec<Vec<u8>> = Vec::new();
                let num_entries:i128 = map.len().try_into()
                    .map_err(|err| format!("AttestationDocument::parse_payload failed to convert pcrs len into i128:{:?}", err))?;
                for x in 0..num_entries {
                    match map.get(&serde_cbor::Value::Integer(x)) {
                        Some(serde_cbor::Value::Bytes(inner_vec)) => {
                            ret_vec.push(inner_vec.to_vec());
                        },
                        _ => return Err("AttestationDocument::parse_payload pcrs inner vec is wrong type or not there?".to_string()),
                    }
                }
                ret_vec
            }
            _ => {
                return Err(
                    "AttestationDocument::parse_payload pcrs is wrong type or not present"
                        .to_string(),
                )
            }
        };

        let nonce: Option<Vec<u8>> =
            match document_map.get(&serde_cbor::Value::Text("nonce".to_string())) {
                Some(serde_cbor::Value::Bytes(val)) => Some(val.to_vec()),
                None => None,
                _ => {
                    return Err(
                        "AttestationDocument::parse_payload nonce is wrong type or not present"
                            .to_string(),
                    )
                }
            };

        let user_data: Option<Vec<u8>> =
            match document_map.get(&serde_cbor::Value::Text("user_data".to_string())) {
                Some(serde_cbor::Value::Bytes(val)) => Some(val.to_vec()),
                None => None,
                Some(_null) => None,
            };

        let digest: String = match document_map.get(&serde_cbor::Value::Text("digest".to_string()))
        {
            Some(serde_cbor::Value::Text(val)) => val.to_string(),
            _ => {
                return Err(
                    "AttestationDocument::parse_payload digest is wrong type or not present"
                        .to_string(),
                )
            }
        };

        let cabundle: Vec<Vec<u8>> =
            match document_map.get(&serde_cbor::Value::Text("cabundle".to_string())) {
                Some(serde_cbor::Value::Array(outer_vec)) => {
                    let mut ret_vec: Vec<Vec<u8>> = Vec::new();
                    for this_vec in outer_vec.iter() {
                        match this_vec {
                            serde_cbor::Value::Bytes(inner_vec) => {
                                ret_vec.push(inner_vec.to_vec());
                            }
                            _ => {
                                return Err(
                                    "AttestationDocument::parse_payload inner_vec is wrong type"
                                        .to_string(),
                                )
                            }
                        }
                    }
                    ret_vec
                }
                _ => {
                    return Err(format!(
                    "AttestationDocument::parse_payload cabundle is wrong type or not present:{:?}",
                    document_map.get(&serde_cbor::Value::Text("cabundle".to_string()))
                ))
                }
            };

        Ok(AttestationDocument {
            module_id,
            timestamp,
            digest,
            pcrs,
            certificate,
            cabundle,
            public_key,
            user_data,
            nonce,
        })
    }
}
