// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::de::{MapAccess, Visitor};
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use x509_parser::public_key::PublicKey;
use x509_parser::time::ASN1Time;
use x509_parser::x509::SubjectPublicKeyInfo;

use crate::error::{SuiError, SuiResult};

use ciborium::value::{Integer, Value};
use once_cell::sync::Lazy;
use p384::ecdsa::signature::Verifier;
use p384::ecdsa::{Signature, VerifyingKey};
use x509_parser::{certificate::X509Certificate, prelude::FromDer};

#[cfg(test)]
#[path = "unit_tests/nitro_attestation_tests.rs"]
mod nitro_attestation_tests;

/// Maximum length of the certificate chain. This is to limit the absolute upper bound on execution.
const MAX_CERT_CHAIN_LENGTH: usize = 10;

/// Root certificate for AWS Nitro Attestation.
static ROOT_CERTIFICATE: Lazy<Vec<u8>> = Lazy::new(|| {
    let pem_bytes = include_bytes!("./nitro_root_certificate.pem");
    let mut pem_cursor = std::io::Cursor::new(pem_bytes);
    let cert = rustls_pemfile::certs(&mut pem_cursor)
        .next()
        .expect("should have root cert")
        .expect("root cert should be valid");
    cert.to_vec()
});

/// Error type for Nitro attestation verification.
#[derive(Debug, PartialEq, Eq)]
pub enum NitroAttestationVerifyError {
    /// Invalid COSE_Sign1: {0}
    InvalidCoseSign1(String),
    /// Invalid signature
    InvalidSignature,
    /// Invalid public key
    InvalidPublicKey,
    /// Siganture failed to verify
    SignatureFailedToVerify,
    /// Invalid attestation document
    InvalidAttestationDoc(String),
    /// Invalid user data.
    InvalidUserData,
    /// Invalid certificate: {0}
    InvalidCertificate(String),
    /// Invalid PCRs
    InvalidPcrs,
}

impl fmt::Display for NitroAttestationVerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NitroAttestationVerifyError::InvalidCoseSign1(msg) => {
                write!(f, "InvalidCoseSign1: {}", msg)
            }
            NitroAttestationVerifyError::InvalidSignature => write!(f, "InvalidSignature"),
            NitroAttestationVerifyError::InvalidPublicKey => write!(f, "InvalidPublicKey"),
            NitroAttestationVerifyError::SignatureFailedToVerify => {
                write!(f, "SignatureFailedToVerify")
            }
            NitroAttestationVerifyError::InvalidAttestationDoc(msg) => {
                write!(f, "InvalidAttestationDoc: {}", msg)
            }
            NitroAttestationVerifyError::InvalidCertificate(msg) => {
                write!(f, "InvalidCertificate: {}", msg)
            }
            NitroAttestationVerifyError::InvalidPcrs => write!(f, "InvalidPcrs"),
            NitroAttestationVerifyError::InvalidUserData => write!(f, "InvalidUserData"),
        }
    }
}

impl From<NitroAttestationVerifyError> for SuiError {
    fn from(err: NitroAttestationVerifyError) -> Self {
        SuiError::AttestationFailedToVerify(err.to_string())
    }
}

/// Given an attestation in bytes, parse it into signature, signed message and a parsed payload.
pub fn parse_nitro_attestation(
    attestation_bytes: &[u8],
) -> SuiResult<(Vec<u8>, Vec<u8>, AttestationDocument)> {
    let cose_sign1 = CoseSign1::parse_and_validate(attestation_bytes)?;
    let doc = AttestationDocument::parse_payload(&cose_sign1.payload)?;
    let signature = cose_sign1.clone().signature;
    Ok((signature, cose_sign1.to_signed_message(), doc))
}

/// Given the signature bytes, signed message and parsed payload, verify everything according to
/// <https://docs.aws.amazon.com/enclaves/latest/user/verify-root.html> and
/// <https://github.com/aws/aws-nitro-enclaves-nsm-api/blob/main/docs/attestation_process.md>.
pub fn verify_nitro_attestation(
    signature: &[u8],
    signed_message: &[u8],
    payload: &AttestationDocument,
    timestamp: u64,
) -> SuiResult<()> {
    // Extract public key from cert and signature as P384.
    let signature = Signature::from_slice(signature)
        .map_err(|_| NitroAttestationVerifyError::InvalidSignature)?;
    let cert = X509Certificate::from_der(payload.certificate.as_slice())
        .map_err(|e| NitroAttestationVerifyError::InvalidCertificate(e.to_string()))?;
    let pk_bytes = SubjectPublicKeyInfo::parsed(cert.1.public_key())
        .map_err(|err| NitroAttestationVerifyError::InvalidCertificate(err.to_string()))?;

    // Verify the signature against the public key and the message.
    match pk_bytes {
        PublicKey::EC(ec) => {
            let verifying_key = VerifyingKey::from_sec1_bytes(ec.data())
                .map_err(|_| NitroAttestationVerifyError::InvalidPublicKey)?;
            verifying_key
                .verify(signed_message, &signature)
                .map_err(|_| NitroAttestationVerifyError::SignatureFailedToVerify)?;
        }
        _ => {
            return Err(NitroAttestationVerifyError::InvalidPublicKey.into());
        }
    }

    payload.verify_cert(timestamp)?;
    Ok(())
}

///  Implementation of the COSE_Sign1 structure as defined in [RFC8152](https://tools.ietf.org/html/rfc8152).
///  protected_header: See Section 3 (Note: AWS Nitro does not have unprotected header.)
///  payload: See Section 4.2.
///  signature: See Section 4.2.
///  Class and trait impl adapted from <https://github.com/awslabs/aws-nitro-enclaves-cose/blob/main/src/sign.rs>
#[derive(Debug, Clone)]
pub struct CoseSign1 {
    /// protected: empty_or_serialized_map,
    protected: Vec<u8>,
    /// unprotected: HeaderMap
    unprotected: HeaderMap,
    /// payload: bstr
    /// The spec allows payload to be nil and transported separately, but it's not useful at the
    /// moment, so this is just a Bytes for simplicity.
    payload: Vec<u8>,
    /// signature: bstr
    signature: Vec<u8>,
}

/// Empty map wrapper for COSE headers.
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
        seq.serialize_element(&Value::Bytes(self.protected.to_vec()))?;
        seq.serialize_element(&self.unprotected)?;
        seq.serialize_element(&Value::Bytes(self.payload.to_vec()))?;
        seq.serialize_element(&Value::Bytes(self.signature.to_vec()))?;
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
                fn extract_bytes(value: Value) -> Option<Vec<u8>> {
                    match value {
                        Value::Bytes(bytes) => Some(bytes),
                        _ => None,
                    }
                }

                // Get protected header bytes
                let protected = match seq.next_element::<Value>()? {
                    Some(v) => extract_bytes(v)
                        .ok_or_else(|| A::Error::custom("protected header must be bytes"))?,
                    None => return Err(A::Error::missing_field("protected")),
                };

                let unprotected = match seq.next_element()? {
                    Some(v) => v,
                    None => return Err(A::Error::missing_field("unprotected")),
                };
                // Get payload bytes
                let payload = match seq.next_element::<Value>()? {
                    Some(v) => {
                        extract_bytes(v).ok_or_else(|| A::Error::custom("payload must be bytes"))?
                    }
                    None => return Err(A::Error::missing_field("payload")),
                };

                // Get signature bytes
                let signature = match seq.next_element::<Value>()? {
                    Some(v) => extract_bytes(v)
                        .ok_or_else(|| A::Error::custom("signature must be bytes"))?,
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
    /// Parse CBOR bytes into struct. Adapted from <https://github.com/awslabs/aws-nitro-enclaves-cose/blob/main/src/sign.rs>
    pub fn parse_and_validate(bytes: &[u8]) -> Result<Self, NitroAttestationVerifyError> {
        let tagged_value: ciborium::value::Value = ciborium::de::from_reader(bytes)
            .map_err(|e| NitroAttestationVerifyError::InvalidCoseSign1(e.to_string()))?;

        let (tag, value) = match tagged_value {
            ciborium::value::Value::Tag(tag, box_value) => (Some(tag), *box_value),
            other => (None, other),
        };

        // Validate tag (18 is the COSE_Sign1 tag)
        match tag {
            None | Some(18) => (),
            Some(_) => {
                return Err(NitroAttestationVerifyError::InvalidCoseSign1(
                    "invalid tag".to_string(),
                ))
            }
        }

        // Create a buffer for serialization
        let mut buf = Vec::new();

        // Serialize the value into the buffer
        ciborium::ser::into_writer(&value, &mut buf)
            .map_err(|e| NitroAttestationVerifyError::InvalidCoseSign1(e.to_string()))?;

        // Deserialize the COSE_Sign1 structure from the buffer
        let cosesign1: Self = ciborium::de::from_reader(&buf[..])
            .map_err(|e| NitroAttestationVerifyError::InvalidCoseSign1(e.to_string()))?;

        // Validate protected header
        let _: HeaderMap = ciborium::de::from_reader(cosesign1.protected.as_slice())
            .map_err(|e| NitroAttestationVerifyError::InvalidCoseSign1(e.to_string()))?;

        cosesign1.validate_header()?;
        Ok(cosesign1)
    }

    /// Validate protected header, payload and signature length.
    pub fn validate_header(&self) -> Result<(), NitroAttestationVerifyError> {
        if !(Self::is_valid_protected_header(self.protected.as_slice())
            && (1..16384).contains(&self.payload.len())
            && self.signature.len() == 96)
        {
            return Err(NitroAttestationVerifyError::InvalidCoseSign1(
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
        let value: Value = match ciborium::de::from_reader(bytes) {
            Ok(v) => v,
            Err(_) => return false,
        };
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

    /// This is the content that the signature is committed over.
    fn to_signed_message(&self) -> Vec<u8> {
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

/// The AWS Nitro Attestation Document, see <https://docs.aws.amazon.com/enclaves/latest/user/verify-root.html#doc-def>
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AttestationDocument {
    pub module_id: String,
    pub timestamp: u64,
    pub digest: String,
    pub pcrs: Vec<Vec<u8>>,
    certificate: Vec<u8>,
    cabundle: Vec<Vec<u8>>,
    pub public_key: Option<Vec<u8>>,
    pub user_data: Option<Vec<u8>>,
    pub nonce: Option<Vec<u8>>,
}

impl AttestationDocument {
    /// Parse the payload of the attestation document, validate the cert based on timestamp, and the pcrs match.
    /// Adapted from <https://github.com/EternisAI/remote-attestation-verifier/blob/main/src/lib.rs>
    pub fn parse_payload(
        payload: &Vec<u8>,
    ) -> Result<AttestationDocument, NitroAttestationVerifyError> {
        let document_data: ciborium::value::Value = ciborium::de::from_reader(payload.as_slice())
            .map_err(|err| {
            NitroAttestationVerifyError::InvalidAttestationDoc(format!(
                "cannot parse payload CBOR: {}",
                err
            ))
        })?;

        let document_map = match document_data {
            ciborium::value::Value::Map(map) => map,
            _ => {
                return Err(NitroAttestationVerifyError::InvalidAttestationDoc(format!(
                    "expected map, got {:?}",
                    document_data
                )))
            }
        };

        let get_string = |key: &str| -> Result<String, NitroAttestationVerifyError> {
            match document_map
                .iter()
                .find(|(k, _)| matches!(k, ciborium::value::Value::Text(s) if s == key))
            {
                Some((_, ciborium::value::Value::Text(val))) => Ok(val.clone()),
                _ => Err(NitroAttestationVerifyError::InvalidAttestationDoc(format!(
                    "cannot parse {}",
                    key
                ))),
            }
        };

        let get_bytes = |key: &str| -> Result<Vec<u8>, NitroAttestationVerifyError> {
            match document_map
                .iter()
                .find(|(k, _)| matches!(k, ciborium::value::Value::Text(s) if s == key))
            {
                Some((_, ciborium::value::Value::Bytes(val))) => Ok(val.clone()),
                _ => Err(NitroAttestationVerifyError::InvalidAttestationDoc(format!(
                    "cannot parse {}",
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
                    NitroAttestationVerifyError::InvalidAttestationDoc(format!(
                        "cannot convert timestamp to u64: {}",
                        err
                    ))
                })?
            }
            _ => {
                return Err(NitroAttestationVerifyError::InvalidAttestationDoc(
                    "cannot parse timestamp".to_string(),
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
                // Valid PCR indices are 0, 1, 2, 3, 4, 8 for AWS.
                for i in [0, 1, 2, 3, 4, 8] {
                    if let Some((_, ciborium::value::Value::Bytes(val))) = pcr_map.iter().find(
                        |(k, _)| matches!(k, ciborium::value::Value::Integer(n) if *n == i.into()),
                    ) {
                        pcr_vec.push(val.clone());
                    } else {
                        return Err(NitroAttestationVerifyError::InvalidAttestationDoc(
                            "invalid PCR format".to_string(),
                        ));
                    }
                }
                pcr_vec
            }
            _ => {
                return Err(NitroAttestationVerifyError::InvalidAttestationDoc(
                    "cannot parse PCRs".to_string(),
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
                    _ => Err(NitroAttestationVerifyError::InvalidAttestationDoc(
                        "invalid cabundle format".to_string(),
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => {
                return Err(NitroAttestationVerifyError::InvalidAttestationDoc(
                    "cannot parse cabundle".to_string(),
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
        Ok(doc)
    }

    /// Verify the certificate against AWS Nitro root of trust and checks expiry.
    fn verify_cert(&self, now: u64) -> Result<(), NitroAttestationVerifyError> {
        // Create chain starting with leaf cert all the way to root.
        let mut chain = Vec::with_capacity(1 + self.cabundle.len());
        chain.push(self.certificate.as_slice());
        chain.extend(self.cabundle.iter().rev().map(|cert| cert.as_slice()));
        verify_cert_chain(&chain, now)
    }

    /// Get the length of the certificate chain.
    pub fn get_cert_chain_length(&self) -> usize {
        self.cabundle.len()
    }
}

/// Verify the certificate chain against the root of trust.
fn verify_cert_chain(cert_chain: &[&[u8]], now_ms: u64) -> Result<(), NitroAttestationVerifyError> {
    if cert_chain.is_empty() || cert_chain.len() > MAX_CERT_CHAIN_LENGTH {
        return Err(NitroAttestationVerifyError::InvalidCertificate(
            "invalid certificate chain length".to_string(),
        ));
    }

    let root_cert = X509Certificate::from_der(ROOT_CERTIFICATE.as_slice())
        .map_err(|e| NitroAttestationVerifyError::InvalidCertificate(e.to_string()))?
        .1;

    let now_secs = ASN1Time::from_timestamp(now_ms as i64 / 1000_i64)
        .map_err(|e| NitroAttestationVerifyError::InvalidCertificate(e.to_string()))?;

    // Validate the chain starting from the leaf
    for i in 0..cert_chain.len() {
        let cert = X509Certificate::from_der(cert_chain[i])
            .map_err(|e| NitroAttestationVerifyError::InvalidCertificate(e.to_string()))?
            .1;

        // Check timestamp validity
        if !cert.validity().is_valid_at(now_secs) {
            return Err(NitroAttestationVerifyError::InvalidCertificate(
                "Certificate timestamp not valid".to_string(),
            ));
        }

        // Get issuer cert from either next in chain or root
        let issuer_cert = if i < cert_chain.len() - 1 {
            X509Certificate::from_der(cert_chain[i + 1])
                .map_err(|e| NitroAttestationVerifyError::InvalidCertificate(e.to_string()))?
                .1
        } else {
            root_cert.clone()
        };

        // Verify issuer/subject chaining
        if cert.issuer() != issuer_cert.subject() {
            return Err(NitroAttestationVerifyError::InvalidCertificate(
                "certificate chain issuer mismatch".to_string(),
            ));
        }

        // Verify signature
        cert.verify_signature(Some(issuer_cert.public_key()))
            .map_err(|_| {
                NitroAttestationVerifyError::InvalidCertificate(
                    "certificate fails to verify".to_string(),
                )
            })?;
    }

    Ok(())
}
