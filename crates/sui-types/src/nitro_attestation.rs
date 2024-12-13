// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::de::{MapAccess, Visitor};
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use serde_bytes::ByteBuf;
use std::fmt;
use std::time::Duration;
use webpki::types::UnixTime;

use crate::error::{SuiError, SuiResult};

use ciborium::value::{Integer, Value};
use p384::ecdsa::signature::Verifier;
use p384::ecdsa::{Signature, VerifyingKey};
use rustls::pki_types::CertificateDer;
use rustls::server::WebPkiClientVerifier;
use rustls::RootCertStore;
use x509_parser::{certificate::X509Certificate, prelude::FromDer};

#[cfg(test)]
#[path = "unit_tests/nitro_attestation_tests.rs"]
mod nitro_attestation_tests;

const ROOT_CERTIFICATE_PEM: &[u8] = include_bytes!("./nitro_root_certificate.pem");
const P384_PUBLIC_KEY_SIZE: usize = 97;

#[derive(Debug, PartialEq, Eq)]
pub enum NitroError {
    /// Invalid COSE_Sign1: {0}
    InvalidCoseSign1(String),
    /// Invalid signature
    InvalidSignature,
    /// Invalid attestation document
    InvalidAttestationDoc(String),
    /// Invalid user data
    InvalidUserData,
    /// Invalid certificate: {0}
    InvalidCertificate(String),
    /// Invalid PCRs
    InvalidPcrs,
}

impl fmt::Display for NitroError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NitroError::InvalidCoseSign1(msg) => write!(f, "InvalidCoseSign1: {}", msg),
            NitroError::InvalidSignature => write!(f, "InvalidSignature"),
            NitroError::InvalidAttestationDoc(msg) => write!(f, "InvalidAttestationDoc: {}", msg),
            NitroError::InvalidCertificate(msg) => write!(f, "InvalidCertificate: {}", msg),
            NitroError::InvalidPcrs => write!(f, "InvalidPcrs"),
            NitroError::InvalidUserData => write!(f, "InvalidUserData"),
        }
    }
}

impl From<NitroError> for SuiError {
    fn from(err: NitroError) -> Self {
        SuiError::AttestationFailedToVerify(err.to_string())
    }
}

/// Given an attestation document bytes, deserialize and verify its validity according to
/// https://docs.aws.amazon.com/enclaves/latest/user/verify-root.html
/// and check the user_data is consistent with the enclave public key.
pub fn attestation_verify_inner(
    attestation_bytes: &[u8],
    enclave_vk: &[u8],
    pcr0: &[u8],
    pcr1: &[u8],
    pcr2: &[u8],
    timestamp: u64,
) -> SuiResult<()> {
    // Parse attestation into a valid cose sign1 object with valid header.
    let cose_sign1 = CoseSign1::parse_and_validate(attestation_bytes)?;

    // Parse attestation document payload and verify cert against AWS root of trust.
    let doc = AttestationDocument::parse_and_validate_payload(
        &cose_sign1.payload,
        timestamp,
        &[pcr0, pcr1, pcr2],
    )?;

    // Extract public key from cert and signature as P384.
    let signature = Signature::from_slice(&cose_sign1.signature).expect("Invalid signature");
    let cert = X509Certificate::from_der(doc.certificate.as_slice())
        .map_err(|e| NitroError::InvalidCertificate(e.to_string()))?;
    let pk_bytes = cert.1.public_key().raw;
    if pk_bytes.len() < P384_PUBLIC_KEY_SIZE {
        return Err(NitroError::InvalidCertificate("Invalid public key length".to_string()).into());
    }
    let public_key = &pk_bytes[pk_bytes.len() - P384_PUBLIC_KEY_SIZE..];
    let verifying_key = VerifyingKey::from_sec1_bytes(public_key)
        .map_err(|err| NitroError::InvalidCertificate(err.to_string()))?;

    // Verify the signature against the public key and the canonical message.
    verifying_key
        .verify(&cose_sign1.to_canonical(), &signature)
        .map_err(|_| NitroError::InvalidSignature)?;

    // Verify the user data equals to the enclave public key.
    let user_data = doc.clone().user_data.ok_or(NitroError::InvalidUserData)?;
    if user_data != enclave_vk {
        return Err(NitroError::InvalidUserData.into());
    }

    // Verify the pcrs.
    doc.validate_pcrs(&[pcr0, pcr1, pcr2])
        .map_err(|_| NitroError::InvalidPcrs)?;
    Ok(())
}

///  Implementation of the COSE_Sign1 structure as defined in [RFC8152](https://tools.ietf.org/html/rfc8152).
///  protected_header: See Section 3 (Note: AWS Nitro does not have unprotected header.)
///  payload: See Section 4.2.
///  signature: See Section 4.2.
///  Class and trait impl adapted from https://github.com/awslabs/aws-nitro-enclaves-cose/blob/main/src/sign.rs
#[derive(Debug, Clone)]
pub struct CoseSign1 {
    /// protected: empty_or_serialized_map,
    protected: ByteBuf,
    /// unprotected: HeaderMap
    unprotected: HeaderMap,
    /// payload: bstr
    /// The spec allows payload to be nil and transported separately, but it's not useful at the
    /// moment, so this is just a ByteBuf for simplicity.
    payload: ByteBuf,
    /// signature: bstr
    signature: ByteBuf,
}

/// Empty map wrapper for COSE headers
#[derive(Clone, Debug, Default)]
pub struct HeaderMap;

impl Serialize for HeaderMap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;
        let map = serializer.serialize_map(Some(0))?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for HeaderMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MapVisitor;

        impl<'de> Visitor<'de> for MapVisitor {
            type Value = HeaderMap;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a map")
            }

            fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                // Consume but ignore any map entries
                while access.next_entry::<Value, Value>()?.is_some() {}
                Ok(HeaderMap)
            }
        }

        deserializer.deserialize_map(MapVisitor)
    }
}

impl Serialize for CoseSign1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(4))?;
        seq.serialize_element(&self.protected)?;
        seq.serialize_element(&self.unprotected)?;
        seq.serialize_element(&self.payload)?;
        seq.serialize_element(&self.signature)?;
        seq.end()
    }
}

impl<'de> Deserialize<'de> for CoseSign1 {
    fn deserialize<D>(deserializer: D) -> Result<CoseSign1, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{Error, SeqAccess, Visitor};
        use std::fmt;

        struct CoseSign1Visitor;

        impl<'de> Visitor<'de> for CoseSign1Visitor {
            type Value = CoseSign1;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a possibly tagged CoseSign1 structure")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<CoseSign1, A::Error>
            where
                A: SeqAccess<'de>,
            {
                // This is the untagged version
                let protected = match seq.next_element()? {
                    Some(v) => v,
                    None => return Err(A::Error::missing_field("protected")),
                };

                let unprotected = match seq.next_element()? {
                    Some(v) => v,
                    None => return Err(A::Error::missing_field("unprotected")),
                };
                let payload = match seq.next_element()? {
                    Some(v) => v,
                    None => return Err(A::Error::missing_field("payload")),
                };
                let signature = match seq.next_element()? {
                    Some(v) => v,
                    None => return Err(A::Error::missing_field("signature")),
                };

                Ok(CoseSign1 {
                    protected,
                    unprotected,
                    payload,
                    signature,
                })
            }

            fn visit_newtype_struct<D>(self, deserializer: D) -> Result<CoseSign1, D::Error>
            where
                D: Deserializer<'de>,
            {
                // This is the tagged version: we ignore the tag part, and just go into it
                deserializer.deserialize_seq(CoseSign1Visitor)
            }
        }

        deserializer.deserialize_any(CoseSign1Visitor)
    }
}

impl CoseSign1 {
    /// Parse CBOR bytes into struct. Adapted from https://github.com/awslabs/aws-nitro-enclaves-cose/blob/main/src/sign.rs
    pub fn parse_and_validate(bytes: &[u8]) -> Result<Self, NitroError> {
        let tagged_value: ciborium::value::Value = ciborium::de::from_reader(bytes)
            .map_err(|e| NitroError::InvalidCoseSign1(e.to_string()))?;

        let (tag, value) = match tagged_value {
            ciborium::value::Value::Tag(tag, box_value) => (Some(tag), *box_value),
            other => (None, other),
        };

        // Validate tag (18 is the COSE_Sign1 tag)
        match tag {
            None | Some(18) => (),
            Some(_) => return Err(NitroError::InvalidCoseSign1("Invalid tag".to_string())),
        }

        // Create a buffer for serialization
        let mut buf = Vec::new();

        // Serialize the value into the buffer
        ciborium::ser::into_writer(&value, &mut buf)
            .map_err(|e| NitroError::InvalidCoseSign1(e.to_string()))?;

        // Deserialize the COSE_Sign1 structure from the buffer
        let cosesign1: Self = ciborium::de::from_reader(&buf[..])
            .map_err(|e| NitroError::InvalidCoseSign1(e.to_string()))?;

        // Validate protected header
        let _: HeaderMap = ciborium::de::from_reader(cosesign1.protected.as_slice())
            .map_err(|e| NitroError::InvalidCoseSign1(e.to_string()))?;

        cosesign1.validate_header()?;
        Ok(cosesign1)
    }

    /// Validate protected header, payload and signature length.
    pub fn validate_header(&self) -> Result<(), NitroError> {
        let is_valid = {
            let mut is_valid = true;
            is_valid &= Self::is_valid_protected_header(self.protected.as_slice());
            is_valid &= (1..16384).contains(&self.payload.len());
            is_valid &= self.signature.len() == 96;
            is_valid
        };
        if !is_valid {
            return Err(NitroError::InvalidCoseSign1(
                "invalid cbor header".to_string(),
            ));
        }
        Ok(())
    }

    // Check protected header: https://docs.aws.amazon.com/enclaves/latest/user/verify-root.html#COSE-CBOR
    // 18(/* COSE_Sign1 CBOR tag is 18 */
    //     {1: -35}, /* This is equivalent with {algorithm: ECDS 384} */
    //     {}, /* We have nothing in unprotected */
    //     $ATTESTATION_DOCUMENT_CONTENT /* Attestation Document */,
    //     signature /* This is the signature */
    // )
    fn is_valid_protected_header(bytes: &[u8]) -> bool {
        let expected_key: Integer = Integer::from(1);
        let expected_val: Integer = Integer::from(-35);
        let value: Value = ciborium::de::from_reader(bytes).expect("valid cbor");
        match value {
            Value::Map(vec) => match &vec[..] {
                [(Value::Integer(key), Value::Integer(val))] => {
                    key == &expected_key && val == &expected_val
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn to_canonical(&self) -> Vec<u8> {
        let value = Value::Array(vec![
            Value::Text("Signature1".to_string()),
            Value::Bytes(self.protected.as_slice().to_vec()),
            Value::Bytes(vec![]),
            Value::Bytes(self.payload.as_slice().to_vec()),
        ]);
        let mut bytes = Vec::with_capacity(self.protected.len() + self.payload.len());
        ciborium::ser::into_writer(&value, &mut bytes).expect("can write bytes");
        bytes
    }
}

/// The AWS Nitro Attestation Document, see https://docs.aws.amazon.com/enclaves/latest/user/verify-root.html#doc-def
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct AttestationDocument {
    module_id: String,
    timestamp: u64,
    digest: String,
    pcrs: Vec<Vec<u8>>,
    certificate: Vec<u8>,
    cabundle: Vec<Vec<u8>>,
    public_key: Option<Vec<u8>>,
    user_data: Option<Vec<u8>>,
    nonce: Option<Vec<u8>>,
}

impl AttestationDocument {
    /// Parse the payload of the attestation document, validate the cert based on timestamp, and the pcrs match. Adapted from https://github.com/EternisAI/remote-attestation-verifier/blob/main/src/lib.rs
    pub fn parse_and_validate_payload(
        payload: &Vec<u8>,
        curr_timestamp: u64,
        expected_pcrs: &[&[u8]],
    ) -> Result<AttestationDocument, NitroError> {
        let document_data: ciborium::value::Value = ciborium::de::from_reader(payload.as_slice())
            .map_err(|err| {
            NitroError::InvalidAttestationDoc(format!("Cannot parse payload CBOR: {}", err))
        })?;

        let document_map = match document_data {
            ciborium::value::Value::Map(map) => map,
            _ => {
                return Err(NitroError::InvalidAttestationDoc(format!(
                    "Expected map, got {:?}",
                    document_data
                )))
            }
        };

        let get_string = |key: &str| -> Result<String, NitroError> {
            match document_map
                .iter()
                .find(|(k, _)| matches!(k, ciborium::value::Value::Text(s) if s == key))
            {
                Some((_, ciborium::value::Value::Text(val))) => Ok(val.clone()),
                _ => Err(NitroError::InvalidAttestationDoc(format!(
                    "Cannot parse {}",
                    key
                ))),
            }
        };

        let get_bytes = |key: &str| -> Result<Vec<u8>, NitroError> {
            match document_map
                .iter()
                .find(|(k, _)| matches!(k, ciborium::value::Value::Text(s) if s == key))
            {
                Some((_, ciborium::value::Value::Bytes(val))) => Ok(val.clone()),
                _ => Err(NitroError::InvalidAttestationDoc(format!(
                    "Cannot parse {}",
                    key
                ))),
            }
        };

        let get_optional_bytes = |key: &str| -> Option<Vec<u8>> {
            document_map
                .iter()
                .find(|(k, _)| matches!(k, ciborium::value::Value::Text(s) if s == key))
                .and_then(|(_, v)| match v {
                    ciborium::value::Value::Bytes(val) => Some(val.clone()),
                    _ => None,
                })
        };

        let module_id = get_string("module_id")?;
        let digest = get_string("digest")?;
        let certificate = get_bytes("certificate")?;

        let timestamp = match document_map
            .iter()
            .find(|(k, _)| matches!(k, ciborium::value::Value::Text(s) if s == "timestamp"))
        {
            Some((_, ciborium::value::Value::Integer(val))) => {
                // Convert Integer to i128 first, then to u64
                let i128_val: i128 = (*val).into();
                u64::try_from(i128_val).map_err(|err| {
                    NitroError::InvalidAttestationDoc(format!(
                        "Cannot convert timestamp to u64: {}",
                        err
                    ))
                })?
            }
            _ => {
                return Err(NitroError::InvalidAttestationDoc(
                    "Cannot parse timestamp".to_string(),
                ))
            }
        };

        let public_key = get_optional_bytes("public_key");

        let pcrs = match document_map
            .iter()
            .find(|(k, _)| matches!(k, ciborium::value::Value::Text(s) if s == "pcrs"))
        {
            Some((_, ciborium::value::Value::Map(pcr_map))) => {
                let mut pcr_vec = Vec::new();
                for i in 0..pcr_map.len() {
                    if let Some((_, ciborium::value::Value::Bytes(val))) = pcr_map.iter().find(
                        |(k, _)| matches!(k, ciborium::value::Value::Integer(n) if *n == i.into()),
                    ) {
                        pcr_vec.push(val.clone());
                    } else {
                        return Err(NitroError::InvalidAttestationDoc(
                            "Invalid PCR format".to_string(),
                        ));
                    }
                }
                pcr_vec
            }
            _ => {
                return Err(NitroError::InvalidAttestationDoc(
                    "Cannot parse PCRs".to_string(),
                ))
            }
        };

        let user_data = get_optional_bytes("user_data");
        let nonce = get_optional_bytes("nonce");

        let cabundle = match document_map
            .iter()
            .find(|(k, _)| matches!(k, ciborium::value::Value::Text(s) if s == "cabundle"))
        {
            Some((_, ciborium::value::Value::Array(arr))) => arr
                .iter()
                .map(|v| match v {
                    ciborium::value::Value::Bytes(bytes) => Ok(bytes.clone()),
                    _ => Err(NitroError::InvalidAttestationDoc(
                        "Invalid cabundle format".to_string(),
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => {
                return Err(NitroError::InvalidAttestationDoc(
                    "Cannot parse cabundle".to_string(),
                ))
            }
        };

        let doc = AttestationDocument {
            module_id,
            timestamp,
            digest,
            pcrs,
            certificate,
            cabundle,
            public_key,
            user_data,
            nonce,
        };

        doc.validate_cert(curr_timestamp)?;
        doc.validate_pcrs(expected_pcrs)?;
        Ok(doc)
    }

    /// Verify the certificate against AWS Nitro root of trust and checks expiry.
    fn validate_cert(&self, now: u64) -> Result<(), NitroError> {
        let mut cert_chain: Vec<CertificateDer<'static>> = Vec::new();
        for ca_cert in self.cabundle.iter().rev() {
            cert_chain.push(CertificateDer::from(ca_cert.to_vec()));
        }

        let leaf_cert = CertificateDer::from(self.certificate.clone());
        cert_chain.push(leaf_cert.clone());

        // Setup root store with AWS Nitro root certificate
        let mut root_store = RootCertStore::empty();
        let mut pem_cursor = std::io::Cursor::new(ROOT_CERTIFICATE_PEM);
        let certs: Vec<_> = rustls_pemfile::certs(&mut pem_cursor)
            .collect::<Result<_, _>>()
            .map_err(|e| {
                NitroError::InvalidCertificate(format!("Failed to parse root certificate: {}", e))
            })?;
        let root_cert = certs
            .into_iter()
            .next()
            .ok_or_else(|| NitroError::InvalidCertificate("No certificates found".to_string()))?;
        root_store
            .add(root_cert)
            .map_err(|e| NitroError::InvalidCertificate(e.to_string()))?;
        let verifier = WebPkiClientVerifier::builder(root_store.into())
            .build()
            .map_err(|e| NitroError::InvalidCertificate(e.to_string()))?;

        verifier
            .verify_client_cert(
                &leaf_cert,
                &cert_chain,
                UnixTime::since_unix_epoch(Duration::from_millis(now)),
            )
            .map_err(|e| NitroError::InvalidCertificate(e.to_string()))?;
        Ok(())
    }

    /// Validate the PCRs against the expected PCRs. todo: add docs
    fn validate_pcrs(&self, expected_pcrs: &[&[u8]]) -> Result<(), NitroError> {
        // only pcr0, pcr1, pcr2 are checked
        assert!(expected_pcrs.len() == 3);
        for (i, expected_pcr) in expected_pcrs.iter().enumerate().take(3) {
            if self.pcrs[i] != *expected_pcr {
                return Err(NitroError::InvalidPcrs);
            }
        }
        Ok(())
    }
}
