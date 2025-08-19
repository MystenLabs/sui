use std::collections::HashMap;
use sui_bcs::Value;

/// Helper trait for extracting fields from struct values
pub trait StructFieldExtractor {
    fn extract_field(&self, name: &str) -> Option<&Value>;
    fn extract_required_field(&self, name: &str) -> Result<&Value, BcsConversionError>;
    fn into_field_map(self) -> HashMap<String, Value>;
}

impl StructFieldExtractor for Vec<(String, Value)> {
    fn extract_field(&self, name: &str) -> Option<&Value> {
        self.iter()
            .find(|(field_name, _)| field_name == name)
            .map(|(_, value)| value)
    }

    fn extract_required_field(&self, name: &str) -> Result<&Value, BcsConversionError> {
        self.extract_field(name)
            .ok_or_else(|| BcsConversionError::MissingField(name.to_string()))
    }

    fn into_field_map(self) -> HashMap<String, Value> {
        self.into_iter().collect()
    }
}

/*
/// Helper for converting Value to primitive types
pub trait ValueConverter: Sized {
    fn from_value(value: &Value, field_name: &str) -> Result<Self, BcsConversionError>;
}

impl ValueConverter for u8 {
    fn from_value(value: &Value, field_name: &str) -> Result<Self, BcsConversionError> {
        match value {
            Value::U8(v) => Ok(*v),
            _ => Err(BcsConversionError::TypeMismatch {
                field: field_name.to_string(),
                expected: "u8".to_string(),
                got: format!("{:?}", value),
            }),
        }
    }
}

impl ValueConverter for u16 {
    fn from_value(value: &Value, field_name: &str) -> Result<Self, BcsConversionError> {
        match value {
            Value::U16(v) => Ok(*v),
            _ => Err(BcsConversionError::TypeMismatch {
                field: field_name.to_string(),
                expected: "u16".to_string(),
                got: format!("{:?}", value),
            }),
        }
    }
}

impl ValueConverter for u32 {
    fn from_value(value: &Value, field_name: &str) -> Result<Self, BcsConversionError> {
        match value {
            Value::U32(v) => Ok(*v),
            _ => Err(BcsConversionError::TypeMismatch {
                field: field_name.to_string(),
                expected: "u32".to_string(),
                got: format!("{:?}", value),
            }),
        }
    }
}

impl ValueConverter for u64 {
    fn from_value(value: &Value, field_name: &str) -> Result<Self, BcsConversionError> {
        match value {
            Value::U64(v) => Ok(*v),
            _ => Err(BcsConversionError::TypeMismatch {
                field: field_name.to_string(),
                expected: "u64".to_string(),
                got: format!("{:?}", value),
            }),
        }
    }
}

impl ValueConverter for u128 {
    fn from_value(value: &Value, field_name: &str) -> Result<Self, BcsConversionError> {
        match value {
            Value::U128(v) => Ok(*v),
            _ => Err(BcsConversionError::TypeMismatch {
                field: field_name.to_string(),
                expected: "u128".to_string(),
                got: format!("{:?}", value),
            }),
        }
    }
}

impl ValueConverter for bool {
    fn from_value(value: &Value, field_name: &str) -> Result<Self, BcsConversionError> {
        match value {
            Value::Bool(v) => Ok(*v),
            _ => Err(BcsConversionError::TypeMismatch {
                field: field_name.to_string(),
                expected: "bool".to_string(),
                got: format!("{:?}", value),
            }),
        }
    }
}

impl ValueConverter for Vec<u8> {
    fn from_value(value: &Value, field_name: &str) -> Result<Self, BcsConversionError> {
        match value {
            Value::Bytes(v) => Ok(v.clone()),
            _ => Err(BcsConversionError::TypeMismatch {
                field: field_name.to_string(),
                expected: "bytes".to_string(),
                got: format!("{:?}", value),
            }),
        }
    }
}

/// Helper for converting Value to Option<T>
pub fn value_to_option<T, F>(
    value: &Value,
    field_name: &str,
    converter: F,
) -> Result<Option<T>, BcsConversionError>
where
    F: FnOnce(&Value) -> Result<T, BcsConversionError>,
{
    match value {
        Value::Seq(values) if values.is_empty() => Ok(None),
        Value::Seq(values) if values.len() == 1 => Ok(Some(converter(&values[0])?)),
        _ => Err(BcsConversionError::TypeMismatch {
            field: field_name.to_string(),
            expected: "Option".to_string(),
            got: format!("{:?}", value),
        }),
    }
}

/// Helper for converting Value to Vec<T>
pub fn value_to_vec<T, F>(
    value: &Value,
    field_name: &str,
    converter: F,
) -> Result<Vec<T>, BcsConversionError>
where
    F: Fn(&Value) -> Result<T, BcsConversionError>,
{
    match value {
        Value::Seq(values) => values.iter().map(converter).collect::<Result<Vec<_>, _>>(),
        _ => Err(BcsConversionError::TypeMismatch {
            field: field_name.to_string(),
            expected: "Seq".to_string(),
            got: format!("{:?}", value),
        }),
    }
}

/// Helper for converting Value to fixed-size array
pub fn value_to_array<const N: usize>(
    value: &Value,
    field_name: &str,
) -> Result<[u8; N], BcsConversionError> {
    match value {
        Value::Array(values) if values.len() == N => {
            let mut result = [0u8; N];
            for (i, v) in values.iter().enumerate() {
                match v {
                    Value::U8(byte) => result[i] = *byte,
                    _ => {
                        return Err(BcsConversionError::TypeMismatch {
                            field: field_name.to_string(),
                            expected: format!("[u8; {}]", N),
                            got: format!("{:?}", value),
                        })
                    }
                }
            }
            Ok(result)
        }
        Value::Bytes(bytes) if bytes.len() == N => {
            let mut result = [0u8; N];
            result.copy_from_slice(bytes);
            Ok(result)
        }
        _ => Err(BcsConversionError::TypeMismatch {
            field: field_name.to_string(),
            expected: format!("[u8; {}]", N),
            got: format!("{:?}", value),
        }),
    }
}

// Add ValueConverter impls for types from sui-types
impl ValueConverter for crate::base_types::ObjectID {
    fn from_value(value: &Value, field_name: &str) -> Result<Self, BcsConversionError> {
        match value {
            Value::Bytes(bytes) if bytes.len() == 32 => {
                let mut addr_bytes = [0u8; 32];
                addr_bytes.copy_from_slice(bytes);
                Ok(crate::base_types::ObjectID::from_address(
                    move_core_types::account_address::AccountAddress::new(addr_bytes),
                ))
            }
            Value::Array(bytes) if bytes.len() == 32 => {
                let mut addr_bytes = [0u8; 32];
                for (i, v) in bytes.iter().enumerate() {
                    match v {
                        Value::U8(byte) => addr_bytes[i] = *byte,
                        _ => {
                            return Err(BcsConversionError::TypeMismatch {
                                field: field_name.to_string(),
                                expected: "address bytes".to_string(),
                                got: format!("{:?}", v),
                            })
                        }
                    }
                }
                Ok(crate::base_types::ObjectID::from_address(
                    move_core_types::account_address::AccountAddress::new(addr_bytes),
                ))
            }
            _ => Err(BcsConversionError::TypeMismatch {
                field: field_name.to_string(),
                expected: "ObjectID".to_string(),
                got: format!("{:?}", value),
            }),
        }
    }
}

impl ValueConverter for crate::base_types::SuiAddress {
    fn from_value(value: &Value, field_name: &str) -> Result<Self, BcsConversionError> {
        match value {
            Value::Bytes(bytes) if bytes.len() == 32 => {
                Ok(crate::base_types::SuiAddress::from_bytes(bytes)
                    .map_err(|e| BcsConversionError::DeserializationError(e.to_string()))?)
            }
            Value::Array(bytes) if bytes.len() == 32 => {
                let mut addr_bytes = [0u8; 32];
                for (i, v) in bytes.iter().enumerate() {
                    match v {
                        Value::U8(byte) => addr_bytes[i] = *byte,
                        _ => {
                            return Err(BcsConversionError::TypeMismatch {
                                field: field_name.to_string(),
                                expected: "address bytes".to_string(),
                                got: format!("{:?}", v),
                            })
                        }
                    }
                }
                Ok(crate::base_types::SuiAddress::from_bytes(addr_bytes)
                    .map_err(|e| BcsConversionError::DeserializationError(e.to_string()))?)
            }
            _ => Err(BcsConversionError::TypeMismatch {
                field: field_name.to_string(),
                expected: "SuiAddress".to_string(),
                got: format!("{:?}", value),
            }),
        }
    }
}

impl ValueConverter for crate::base_types::SequenceNumber {
    fn from_value(value: &Value, field_name: &str) -> Result<Self, BcsConversionError> {
        match value {
            Value::U64(v) => Ok(crate::base_types::SequenceNumber::from_u64(*v)),
            _ => Err(BcsConversionError::TypeMismatch {
                field: field_name.to_string(),
                expected: "SequenceNumber".to_string(),
                got: format!("{:?}", value),
            }),
        }
    }
}

impl ValueConverter for crate::digests::ObjectDigest {
    fn from_value(value: &Value, field_name: &str) -> Result<Self, BcsConversionError> {
        match value {
            Value::Struct(fields) => {
                // ObjectDigest is a NEWTYPESTRUCT wrapping Digest
                let digest_field = fields.extract_required_field("digest")?;
                match digest_field {
                    Value::Array(bytes) if bytes.len() == 32 => {
                        let mut digest_bytes = [0u8; 32];
                        for (i, v) in bytes.iter().enumerate() {
                            match v {
                                Value::U8(byte) => digest_bytes[i] = *byte,
                                _ => {
                                    return Err(BcsConversionError::TypeMismatch {
                                        field: field_name.to_string(),
                                        expected: "digest bytes".to_string(),
                                        got: format!("{:?}", v),
                                    })
                                }
                            }
                        }
                        Ok(crate::digests::ObjectDigest::new(digest_bytes))
                    }
                    _ => Err(BcsConversionError::TypeMismatch {
                        field: field_name.to_string(),
                        expected: "32-byte digest".to_string(),
                        got: format!("{:?}", digest_field),
                    }),
                }
            }
            _ => Err(BcsConversionError::TypeMismatch {
                field: field_name.to_string(),
                expected: "ObjectDigest struct".to_string(),
                got: format!("{:?}", value),
            }),
        }
    }
}

impl ValueConverter for crate::digests::TransactionDigest {
    fn from_value(value: &Value, field_name: &str) -> Result<Self, BcsConversionError> {
        match value {
            Value::Struct(fields) => {
                let digest_field = fields.extract_required_field("digest")?;
                match digest_field {
                    Value::Array(bytes) if bytes.len() == 32 => {
                        let mut digest_bytes = [0u8; 32];
                        for (i, v) in bytes.iter().enumerate() {
                            match v {
                                Value::U8(byte) => digest_bytes[i] = *byte,
                                _ => {
                                    return Err(BcsConversionError::TypeMismatch {
                                        field: field_name.to_string(),
                                        expected: "digest bytes".to_string(),
                                        got: format!("{:?}", v),
                                    })
                                }
                            }
                        }
                        Ok(crate::digests::TransactionDigest::new(digest_bytes))
                    }
                    _ => Err(BcsConversionError::TypeMismatch {
                        field: field_name.to_string(),
                        expected: "32-byte digest".to_string(),
                        got: format!("{:?}", digest_field),
                    }),
                }
            }
            _ => Err(BcsConversionError::TypeMismatch {
                field: field_name.to_string(),
                expected: "TransactionDigest struct".to_string(),
                got: format!("{:?}", value),
            }),
        }
    }
}

impl ValueConverter for crate::digests::CheckpointDigest {
    fn from_value(value: &Value, field_name: &str) -> Result<Self, BcsConversionError> {
        match value {
            Value::Struct(fields) => {
                let digest_field = fields.extract_required_field("digest")?;
                match digest_field {
                    Value::Array(bytes) if bytes.len() == 32 => {
                        let mut digest_bytes = [0u8; 32];
                        for (i, v) in bytes.iter().enumerate() {
                            match v {
                                Value::U8(byte) => digest_bytes[i] = *byte,
                                _ => {
                                    return Err(BcsConversionError::TypeMismatch {
                                        field: field_name.to_string(),
                                        expected: "digest bytes".to_string(),
                                        got: format!("{:?}", v),
                                    })
                                }
                            }
                        }
                        Ok(crate::digests::CheckpointDigest::new(digest_bytes))
                    }
                    _ => Err(BcsConversionError::TypeMismatch {
                        field: field_name.to_string(),
                        expected: "32-byte digest".to_string(),
                        got: format!("{:?}", digest_field),
                    }),
                }
            }
            _ => Err(BcsConversionError::TypeMismatch {
                field: field_name.to_string(),
                expected: "CheckpointDigest struct".to_string(),
                got: format!("{:?}", value),
            }),
        }
    }
}

impl ValueConverter for crate::digests::CheckpointContentsDigest {
    fn from_value(value: &Value, field_name: &str) -> Result<Self, BcsConversionError> {
        match value {
            Value::Struct(fields) => {
                let digest_field = fields.extract_required_field("digest")?;
                match digest_field {
                    Value::Array(bytes) if bytes.len() == 32 => {
                        let mut digest_bytes = [0u8; 32];
                        for (i, v) in bytes.iter().enumerate() {
                            match v {
                                Value::U8(byte) => digest_bytes[i] = *byte,
                                _ => {
                                    return Err(BcsConversionError::TypeMismatch {
                                        field: field_name.to_string(),
                                        expected: "digest bytes".to_string(),
                                        got: format!("{:?}", v),
                                    })
                                }
                            }
                        }
                        Ok(crate::digests::CheckpointContentsDigest::new(digest_bytes))
                    }
                    _ => Err(BcsConversionError::TypeMismatch {
                        field: field_name.to_string(),
                        expected: "32-byte digest".to_string(),
                        got: format!("{:?}", digest_field),
                    }),
                }
            }
            _ => Err(BcsConversionError::TypeMismatch {
                field: field_name.to_string(),
                expected: "CheckpointContentsDigest struct".to_string(),
                got: format!("{:?}", value),
            }),
        }
    }
}
*/

#[cfg(test)]
#[path = "unit_tests/bcs_value_converter_tests.rs"]
mod bcs_value_converter_tests;

#[cfg(test)]
mod tests {
    use crate::gas::GasCostSummary;
    use std::convert::TryFrom;

    #[test]
    fn test_gas_cost_summary_round_trip() {
        let gas_cost = GasCostSummary {
            computation_cost: 1000,
            storage_cost: 2000,
            storage_rebate: 500,
            non_refundable_storage_fee: 100,
        };

        // Serialize to BCS bytes
        let encoded = bcs::to_bytes(&gas_cost).expect("Failed to serialize to BCS");

        // Parse with sui-bcs to get Value
        let yaml_content = include_str!("../../sui-core/tests/staged/sui.yaml");
        let parser = sui_bcs::Parser::from_yaml(yaml_content).expect("Failed to parse YAML");

        let value = parser
            .parse(&encoded, "GasCostSummary")
            .expect("Failed to parse BCS data");

        // Convert Value back to Rust type
        let converted = GasCostSummary::try_from(value).expect("Failed to convert from Value");

        // Check equality
        assert_eq!(gas_cost, converted, "Round-trip conversion failed");
    }
}
