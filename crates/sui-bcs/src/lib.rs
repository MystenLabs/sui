pub use bcs;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TypeLayout {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    Bytes,
    Unit,
    NewtypeStruct(Box<TypeLayout>),
    Struct(Vec<(String, TypeLayout)>),
    Enum(HashMap<u32, EnumVariant>),
    Seq(Box<TypeLayout>),
    Tuple(Vec<TypeLayout>),
    TupleArray {
        content: Box<TypeLayout>,
        size: usize,
    },
    Option(Box<TypeLayout>),
    TypeName(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumVariant {
    pub name: String,
    pub layout: TypeLayout,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    Bytes(Vec<u8>),
    Unit,
    Struct(Vec<(String, Value)>),
    Enum(String, Box<Value>),
    Seq(Vec<Value>),
    Tuple(Vec<Value>),
    Array(Vec<Value>),
}

pub struct Parser {
    schemas: HashMap<String, TypeLayout>,
}

impl Parser {
    pub fn from_yaml(yaml_content: &str) -> Result<Self> {
        let raw_schemas: HashMap<String, serde_yaml::Value> =
            serde_yaml::from_str(yaml_content).context("Failed to parse YAML content")?;
        let mut schemas = HashMap::new();

        for (name, value) in raw_schemas {
            match Self::parse_layout(&value) {
                Ok(layout) => {
                    schemas.insert(name, layout);
                }
                Err(_e) => {
                    // Skip types we can't parse yet
                    // eprintln!("Failed to parse type {}: {}", name, e);
                }
            }
        }

        Ok(Parser { schemas })
    }

    fn parse_layout(value: &serde_yaml::Value) -> Result<TypeLayout> {
        use serde_yaml::Value;

        match value {
            Value::String(s) => match s.as_str() {
                "BOOL" => Ok(TypeLayout::Bool),
                "U8" => Ok(TypeLayout::U8),
                "U16" => Ok(TypeLayout::U16),
                "U32" => Ok(TypeLayout::U32),
                "U64" => Ok(TypeLayout::U64),
                "U128" => Ok(TypeLayout::U128),
                "BYTES" => Ok(TypeLayout::Bytes),
                "UNIT" => Ok(TypeLayout::Unit),
                _ => Err(anyhow!("Unknown type: {}", s)),
            },
            Value::Mapping(map) => {
                if let Some(newtype) = map.get(Value::String("NEWTYPESTRUCT".to_string())) {
                    match newtype {
                        Value::String(s) if s == "BYTES" => {
                            Ok(TypeLayout::NewtypeStruct(Box::new(TypeLayout::Bytes)))
                        }
                        Value::Mapping(inner) => {
                            if let Some(tuple_array) =
                                inner.get(Value::String("TUPLEARRAY".to_string()))
                            {
                                if let Value::Mapping(ta_map) = tuple_array {
                                    let content = ta_map
                                        .get(Value::String("CONTENT".to_string()))
                                        .and_then(|v| Self::parse_layout(v).ok())
                                        .ok_or_else(|| anyhow!("Missing CONTENT in TUPLEARRAY"))?;
                                    let size = ta_map
                                        .get(Value::String("SIZE".to_string()))
                                        .and_then(|v| v.as_u64())
                                        .ok_or_else(|| anyhow!("Missing SIZE in TUPLEARRAY"))?
                                        as usize;
                                    Ok(TypeLayout::NewtypeStruct(Box::new(
                                        TypeLayout::TupleArray {
                                            content: Box::new(content),
                                            size,
                                        },
                                    )))
                                } else {
                                    Err(anyhow!("Invalid TUPLEARRAY format"))
                                }
                            } else if let Some(typename) =
                                inner.get(Value::String("TYPENAME".to_string()))
                            {
                                if let Value::String(name) = typename {
                                    Ok(TypeLayout::NewtypeStruct(Box::new(TypeLayout::TypeName(
                                        name.clone(),
                                    ))))
                                } else {
                                    Err(anyhow!("Invalid TYPENAME format"))
                                }
                            } else {
                                Self::parse_layout(newtype)
                                    .map(|l| TypeLayout::NewtypeStruct(Box::new(l)))
                            }
                        }
                        _ => Self::parse_layout(newtype)
                            .map(|l| TypeLayout::NewtypeStruct(Box::new(l))),
                    }
                } else if let Some(struct_fields) = map.get(Value::String("STRUCT".to_string())) {
                    if let Value::Sequence(fields) = struct_fields {
                        let mut parsed_fields = Vec::new();
                        for field in fields {
                            if let Value::Mapping(field_map) = field {
                                for (key, val) in field_map {
                                    if let Value::String(field_name) = key {
                                        let field_type = Self::parse_layout(val)?;
                                        parsed_fields.push((field_name.clone(), field_type));
                                    }
                                }
                            }
                        }
                        Ok(TypeLayout::Struct(parsed_fields))
                    } else {
                        Err(anyhow!("Invalid STRUCT format"))
                    }
                } else if let Some(enum_variants) = map.get(Value::String("ENUM".to_string())) {
                    if let Value::Mapping(variants_map) = enum_variants {
                        let mut parsed_variants = HashMap::new();
                        for (key, val) in variants_map {
                            if let Value::Number(idx) = key {
                                let index =
                                    idx.as_u64().ok_or_else(|| anyhow!("Invalid enum index"))?
                                        as u32;
                                if let Value::Mapping(variant_map) = val {
                                    for (name, layout) in variant_map {
                                        if let Value::String(variant_name) = name {
                                            // Check if this is a NEWTYPE variant
                                            let variant_layout =
                                                if let Value::Mapping(inner_map) = layout {
                                                    if let Some(newtype_val) = inner_map
                                                        .get(Value::String("NEWTYPE".to_string()))
                                                    {
                                                        // This is a NEWTYPE variant, parse its inner type
                                                        Self::parse_layout(newtype_val)?
                                                    } else {
                                                        // Not a NEWTYPE, parse as normal
                                                        Self::parse_layout(layout)?
                                                    }
                                                } else {
                                                    Self::parse_layout(layout)?
                                                };
                                            parsed_variants.insert(
                                                index,
                                                EnumVariant {
                                                    name: variant_name.clone(),
                                                    layout: variant_layout,
                                                },
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        Ok(TypeLayout::Enum(parsed_variants))
                    } else {
                        Err(anyhow!("Invalid ENUM format"))
                    }
                } else if let Some(seq) = map.get(Value::String("SEQ".to_string())) {
                    Self::parse_layout(seq).map(|l| TypeLayout::Seq(Box::new(l)))
                } else if let Some(option) = map.get(Value::String("OPTION".to_string())) {
                    Self::parse_layout(option).map(|l| TypeLayout::Option(Box::new(l)))
                } else if let Some(tuple) = map.get(Value::String("TUPLE".to_string())) {
                    if let Value::Sequence(elements) = tuple {
                        let mut parsed_elements = Vec::new();
                        for elem in elements {
                            parsed_elements.push(Self::parse_layout(elem)?);
                        }
                        Ok(TypeLayout::Tuple(parsed_elements))
                    } else {
                        Err(anyhow!("Invalid TUPLE format"))
                    }
                } else if let Some(typename) = map.get(Value::String("TYPENAME".to_string())) {
                    if let Value::String(name) = typename {
                        Ok(TypeLayout::TypeName(name.clone()))
                    } else {
                        Err(anyhow!("Invalid TYPENAME format"))
                    }
                } else {
                    Err(anyhow!("Unknown mapping type in YAML schema"))
                }
            }
            _ => Err(anyhow!("Invalid layout format in YAML schema")),
        }
    }

    pub fn parse(&self, data: &[u8], type_name: &str) -> Result<Value> {
        let layout = self
            .schemas
            .get(type_name)
            .ok_or_else(|| anyhow!("Type '{}' not found in schema", type_name))?;
        let mut cursor = std::io::Cursor::new(data);
        self.parse_value(&mut cursor, layout)
    }

    pub fn parse_value(
        &self,
        cursor: &mut std::io::Cursor<&[u8]>,
        layout: &TypeLayout,
    ) -> Result<Value> {
        use std::io::Read;

        match layout {
            TypeLayout::Bool => {
                let mut buf = [0u8; 1];
                cursor.read_exact(&mut buf).context("Failed to read bool")?;
                Ok(Value::Bool(buf[0] != 0))
            }
            TypeLayout::U8 => {
                let mut buf = [0u8; 1];
                cursor.read_exact(&mut buf).context("Failed to read u8")?;
                Ok(Value::U8(buf[0]))
            }
            TypeLayout::U16 => {
                let mut buf = [0u8; 2];
                cursor.read_exact(&mut buf).context("Failed to read u16")?;
                Ok(Value::U16(u16::from_le_bytes(buf)))
            }
            TypeLayout::U32 => {
                let mut buf = [0u8; 4];
                cursor.read_exact(&mut buf).context("Failed to read u32")?;
                Ok(Value::U32(u32::from_le_bytes(buf)))
            }
            TypeLayout::U64 => {
                let mut buf = [0u8; 8];
                cursor.read_exact(&mut buf).context("Failed to read u64")?;
                Ok(Value::U64(u64::from_le_bytes(buf)))
            }
            TypeLayout::U128 => {
                let mut buf = [0u8; 16];
                cursor.read_exact(&mut buf).context("Failed to read u128")?;
                Ok(Value::U128(u128::from_le_bytes(buf)))
            }
            TypeLayout::Bytes => {
                let len = self.read_uleb128(cursor)?;
                let mut buf = vec![0u8; len as usize];
                cursor
                    .read_exact(&mut buf)
                    .context("Failed to read bytes")?;
                Ok(Value::Bytes(buf))
            }
            TypeLayout::Unit => Ok(Value::Unit),
            TypeLayout::NewtypeStruct(inner) => self.parse_value(cursor, inner),
            TypeLayout::Struct(fields) => {
                let mut values = Vec::new();
                for (name, field_layout) in fields {
                    let value = self.parse_value(cursor, field_layout)?;
                    values.push((name.clone(), value));
                }
                Ok(Value::Struct(values))
            }
            TypeLayout::Enum(variants) => {
                let variant_idx = self.read_uleb128(cursor)?;
                let variant = variants
                    .get(&(variant_idx as u32))
                    .ok_or_else(|| anyhow!("Unknown variant index: {}", variant_idx))?;
                let value = self.parse_value(cursor, &variant.layout)?;
                Ok(Value::Enum(variant.name.clone(), Box::new(value)))
            }
            TypeLayout::Seq(elem_layout) => {
                let len = self.read_uleb128(cursor)?;
                let mut values = Vec::new();
                for _ in 0..len {
                    values.push(self.parse_value(cursor, elem_layout)?);
                }
                Ok(Value::Seq(values))
            }
            TypeLayout::Tuple(elem_layouts) => {
                let mut values = Vec::new();
                for layout in elem_layouts {
                    values.push(self.parse_value(cursor, layout)?);
                }
                Ok(Value::Tuple(values))
            }
            TypeLayout::TupleArray { content, size } => {
                let mut values = Vec::new();
                for _ in 0..*size {
                    values.push(self.parse_value(cursor, content)?);
                }
                Ok(Value::Array(values))
            }
            TypeLayout::Option(inner) => {
                use std::io::Read;
                let mut buf = [0u8; 1];
                cursor
                    .read_exact(&mut buf)
                    .context("Failed to read Option tag")?;
                if buf[0] == 0 {
                    Ok(Value::Seq(vec![])) // None represented as empty seq
                } else if buf[0] == 1 {
                    let value = self.parse_value(cursor, inner)?;
                    Ok(Value::Seq(vec![value])) // Some represented as single-element seq
                } else {
                    Err(anyhow!("Invalid Option tag: {}", buf[0]))
                }
            }
            TypeLayout::TypeName(name) => {
                let layout = self
                    .schemas
                    .get(name)
                    .ok_or_else(|| anyhow!("Type '{}' not found in schema", name))?;
                self.parse_value(cursor, layout)
            }
        }
    }

    pub fn read_uleb128(&self, cursor: &mut std::io::Cursor<&[u8]>) -> Result<u64> {
        use std::io::Read;

        let mut value = 0u64;
        let mut shift = 0;
        loop {
            let mut buf = [0u8; 1];
            cursor
                .read_exact(&mut buf)
                .context("Failed to read ULEB128 byte")?;
            let byte = buf[0];
            value |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > 63 {
                return Err(anyhow!("ULEB128 value too large (> 63 bits)"));
            }
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_digest() {
        let yaml_content = include_str!("../../sui-core/tests/staged/sui.yaml");
        let parser = Parser::from_yaml(yaml_content).unwrap();

        use sui_types::digests::Digest;
        let digest = Digest::random();
        let encoded = bcs::to_bytes(&digest).unwrap();

        let parsed = parser.parse(&encoded, "Digest").unwrap();

        if let Value::Bytes(bytes) = parsed {
            assert_eq!(bytes.len(), 32);
            let decoded_digest: Digest = bcs::from_bytes(&encoded).unwrap();
            assert_eq!(digest, decoded_digest);
        } else {
            panic!("Expected Bytes value for Digest");
        }
    }

    #[test]
    fn test_execution_status_enum() {
        let yaml_content = include_str!("../../sui-core/tests/staged/sui.yaml");
        let parser = Parser::from_yaml(yaml_content).unwrap();

        use sui_types::execution_status::ExecutionStatus;

        // Test Success variant (unit variant)
        let status = ExecutionStatus::Success;
        let encoded = bcs::to_bytes(&status).unwrap();

        let parsed = parser.parse(&encoded, "ExecutionStatus").unwrap();

        if let Value::Enum(variant_name, variant_value) = parsed {
            assert_eq!(variant_name, "Success");
            if let Value::Unit = *variant_value {
                // Success variant has unit value
            } else {
                panic!("Expected Unit value for Success variant");
            }
        } else {
            panic!("Expected Enum value for ExecutionStatus");
        }
    }

    #[test]
    fn test_gas_cost_summary() {
        let yaml_content = include_str!("../../sui-core/tests/staged/sui.yaml");
        let parser = Parser::from_yaml(yaml_content).unwrap();

        use sui_types::gas::GasCostSummary;
        let gas_cost = GasCostSummary {
            computation_cost: 1000,
            storage_cost: 2000,
            storage_rebate: 500,
            non_refundable_storage_fee: 100,
        };
        let encoded = bcs::to_bytes(&gas_cost).unwrap();

        let parsed = parser.parse(&encoded, "GasCostSummary").unwrap();

        if let Value::Struct(fields) = parsed {
            assert_eq!(fields.len(), 4);

            let mut found_computation = false;
            let mut found_storage = false;
            let mut found_rebate = false;
            let mut found_non_refundable = false;

            for (name, value) in fields {
                match name.as_str() {
                    "computation_cost" => {
                        if let Value::U64(v) = value {
                            assert_eq!(v, 1000);
                            found_computation = true;
                        }
                    }
                    "storage_cost" => {
                        if let Value::U64(v) = value {
                            assert_eq!(v, 2000);
                            found_storage = true;
                        }
                    }
                    "storage_rebate" => {
                        if let Value::U64(v) = value {
                            assert_eq!(v, 500);
                            found_rebate = true;
                        }
                    }
                    "non_refundable_storage_fee" => {
                        if let Value::U64(v) = value {
                            assert_eq!(v, 100);
                            found_non_refundable = true;
                        }
                    }
                    _ => panic!("Unexpected field: {}", name),
                }
            }

            assert!(found_computation && found_storage && found_rebate && found_non_refundable);
        } else {
            panic!("Expected Struct value for GasCostSummary");
        }
    }

    #[test]
    fn test_sui_address() {
        let yaml_content = include_str!("../../sui-core/tests/staged/sui.yaml");
        let parser = Parser::from_yaml(yaml_content).unwrap();

        use sui_types::base_types::SuiAddress;

        // Test SuiAddress which is an alias for a fixed array
        let address = SuiAddress::random_for_testing_only();
        let encoded = bcs::to_bytes(&address).unwrap();
        let parsed = parser.parse(&encoded, "SuiAddress").unwrap();

        // SuiAddress is a newtype struct around an array
        if let Value::Array(bytes) = parsed {
            assert_eq!(bytes.len(), 32); // SuiAddress is 32 bytes
        } else {
            panic!("Expected Array value for SuiAddress");
        }
    }

    #[test]
    fn test_sequence_type() {
        let yaml_content = include_str!("../../sui-core/tests/staged/sui.yaml");
        let parser = Parser::from_yaml(yaml_content).unwrap();

        // Test a simple vector of digests
        use sui_types::digests::Digest;

        let digests = vec![Digest::random(), Digest::random()];
        let encoded = bcs::to_bytes(&digests).unwrap();

        // Parse as a sequence by building the layout manually
        let seq_layout = TypeLayout::Seq(Box::new(TypeLayout::TypeName("Digest".to_string())));
        let mut cursor = std::io::Cursor::new(&encoded[..]);
        let parsed = parser.parse_value(&mut cursor, &seq_layout).unwrap();

        if let Value::Seq(values) = parsed {
            assert_eq!(values.len(), 2);
            for value in values {
                if let Value::Bytes(bytes) = value {
                    assert_eq!(bytes.len(), 32); // Digest is 32 bytes
                } else {
                    panic!("Expected Bytes value for Digest in sequence");
                }
            }
        } else {
            panic!("Expected Seq value");
        }
    }

    #[test]
    fn test_transaction_effects() {
        let yaml_content = include_str!("../../sui-core/tests/staged/sui.yaml");
        let parser = Parser::from_yaml(yaml_content).unwrap();

        use sui_types::digests::TransactionDigest;
        use sui_types::effects::{TransactionEffects, TransactionEffectsV2};
        use sui_types::execution_status::ExecutionStatus;

        // Create a minimal TransactionEffectsV2
        let effects_v2 = TransactionEffectsV2::new(
            ExecutionStatus::Success,
            42,                          // executed_epoch
            Default::default(),          // gas_used
            vec![],                      // shared_objects
            Default::default(),          // loaded_per_epoch_config_objects
            TransactionDigest::random(), // transaction_digest
            Default::default(),          // lamport_version
            Default::default(),          // changed_objects
            None,                        // gas_object
            None,                        // events_digest
            vec![],                      // dependencies
        );

        let effects = TransactionEffects::V2(effects_v2);
        let encoded = bcs::to_bytes(&effects).unwrap();
        let parsed = parser.parse(&encoded, "TransactionEffects").unwrap();

        // TransactionEffects is an enum with V1 and V2 variants
        if let Value::Enum(variant_name, _variant_value) = parsed {
            assert_eq!(variant_name, "V2");
        } else {
            panic!("Expected Enum value for TransactionEffects");
        }
    }

    #[test]
    fn test_transaction_data() {
        let yaml_content = include_str!("../../sui-core/tests/staged/sui.yaml");
        let parser = Parser::from_yaml(yaml_content).unwrap();

        use sui_types::base_types::{ObjectDigest, ObjectID, SuiAddress};
        use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
        use sui_types::transaction::{
            CallArg, GasData, ObjectArg, TransactionExpiration, TransactionKind,
        };
        use sui_types::transaction::{TransactionData, TransactionDataV1};

        // Create a TransactionData with inputs and commands
        let mut ptb = ProgrammableTransactionBuilder::new();

        // Add some inputs
        let input1 = CallArg::Pure(vec![1, 2, 3, 4]); // Pure bytes input
        let input2 = CallArg::Object(ObjectArg::ImmOrOwnedObject((
            ObjectID::random(),
            Default::default(),
            ObjectDigest::random(),
        )));

        let _ = ptb.input(input1);
        let _ = ptb.input(input2);

        // Add some transfer commands
        let obj_arg = ptb
            .obj(ObjectArg::ImmOrOwnedObject((
                ObjectID::random(),
                Default::default(),
                ObjectDigest::random(),
            )))
            .unwrap();
        let recipient = SuiAddress::random_for_testing_only();
        ptb.transfer_arg(recipient, obj_arg);

        // Add another transfer to have multiple commands
        let obj_arg2 = ptb
            .obj(ObjectArg::ImmOrOwnedObject((
                ObjectID::random(),
                Default::default(),
                ObjectDigest::random(),
            )))
            .unwrap();
        ptb.transfer_arg(recipient, obj_arg2);

        let gas_data = GasData {
            payment: vec![(
                ObjectID::random(),
                Default::default(),
                ObjectDigest::random(),
            )],
            owner: SuiAddress::random_for_testing_only(),
            price: 1000,
            budget: 10000,
        };

        let tx_data_v1 = TransactionDataV1 {
            kind: TransactionKind::ProgrammableTransaction(ptb.finish()),
            sender: SuiAddress::random_for_testing_only(),
            gas_data,
            expiration: TransactionExpiration::None,
        };

        let tx_data = TransactionData::V1(tx_data_v1);
        let encoded = bcs::to_bytes(&tx_data).unwrap();
        let parsed = parser.parse(&encoded, "TransactionData").unwrap();

        // TransactionData is an enum with V1 variant
        if let Value::Enum(variant_name, variant_value) = parsed {
            assert_eq!(variant_name, "V1");
            // V1 contains a struct (TransactionDataV1)
            if let Value::Struct(fields) = *variant_value {
                // Check that essential fields exist
                let has_kind = fields.iter().any(|(name, _)| name == "kind");
                let has_sender = fields.iter().any(|(name, _)| name == "sender");
                let has_gas_data = fields.iter().any(|(name, _)| name == "gas_data");
                assert!(has_kind, "TransactionDataV1 should have 'kind' field");
                assert!(has_sender, "TransactionDataV1 should have 'sender' field");
                assert!(
                    has_gas_data,
                    "TransactionDataV1 should have 'gas_data' field"
                );

                // Check that the kind field is a ProgrammableTransaction with inputs and commands
                let kind_field = fields.iter().find(|(name, _)| name == "kind").unwrap();
                if let (_, Value::Enum(kind_variant, kind_value)) = kind_field {
                    assert_eq!(kind_variant, "ProgrammableTransaction");
                    // The ProgrammableTransaction should contain inputs and commands
                    if let Value::Struct(pt_fields) = &**kind_value {
                        let has_inputs = pt_fields.iter().any(|(name, _)| name == "inputs");
                        let has_commands = pt_fields.iter().any(|(name, _)| name == "commands");
                        assert!(
                            has_inputs,
                            "ProgrammableTransaction should have 'inputs' field"
                        );
                        assert!(
                            has_commands,
                            "ProgrammableTransaction should have 'commands' field"
                        );

                        // Verify we have inputs
                        let inputs_field =
                            pt_fields.iter().find(|(name, _)| name == "inputs").unwrap();
                        if let (_, Value::Seq(inputs)) = inputs_field {
                            assert!(inputs.len() >= 2, "Should have at least 2 inputs");
                        }

                        // Verify we have commands
                        let commands_field = pt_fields
                            .iter()
                            .find(|(name, _)| name == "commands")
                            .unwrap();
                        if let (_, Value::Seq(commands)) = commands_field {
                            assert!(commands.len() >= 2, "Should have at least 2 commands");
                        }
                    }
                }
            } else {
                panic!("Expected Struct value for TransactionDataV1");
            }
        } else {
            panic!("Expected Enum value for TransactionData");
        }
    }

    #[test]
    fn test_transaction_round_trip() {
        use sui_types::base_types::*;
        use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
        use sui_types::transaction::GasData;
        use sui_types::transaction::*;

        let yaml_content = include_str!("../../sui-core/tests/staged/sui.yaml");
        let parser = Parser::from_yaml(yaml_content).unwrap();

        // Create a test transaction similar to the parsing test
        let mut ptb = ProgrammableTransactionBuilder::new();
        let recipient = SuiAddress::random_for_testing_only();
        let obj_arg = ptb
            .obj(ObjectArg::ImmOrOwnedObject((
                ObjectID::random(),
                Default::default(),
                ObjectDigest::random(),
            )))
            .unwrap();
        ptb.transfer_arg(recipient, obj_arg);

        let gas_data = GasData {
            payment: vec![(
                ObjectID::random(),
                Default::default(),
                ObjectDigest::random(),
            )],
            owner: SuiAddress::random_for_testing_only(),
            price: 1000,
            budget: 10000,
        };

        let tx_data_v1 = TransactionDataV1 {
            kind: TransactionKind::ProgrammableTransaction(ptb.finish()),
            sender: SuiAddress::random_for_testing_only(),
            gas_data,
            expiration: TransactionExpiration::None,
        };

        let tx_data = TransactionData::V1(tx_data_v1);

        // Round-trip test: encode -> parse -> convert back
        let encoded = bcs::to_bytes(&tx_data).unwrap();
        let parsed = parser.parse(&encoded, "TransactionData").unwrap();
        let converted_back = TransactionData::try_from(parsed).unwrap();

        assert_eq!(tx_data, converted_back);
    }

    #[test]
    fn test_transaction_effects_round_trip() {
        use sui_types::effects::*;

        let yaml_content = include_str!("../../sui-core/tests/staged/sui.yaml");
        let parser = Parser::from_yaml(yaml_content).unwrap();

        // Create test TransactionEffectsV2
        let effects_v2 = TransactionEffectsV2::default();
        let effects = TransactionEffects::V2(effects_v2);

        // Round-trip test: encode -> parse -> convert back
        let encoded = bcs::to_bytes(&effects).unwrap();
        let parsed = parser.parse(&encoded, "TransactionEffects").unwrap();
        let converted_back = TransactionEffects::try_from(parsed).unwrap();

        assert_eq!(effects, converted_back);
    }

    #[test]
    fn test_checkpoint_summary_round_trip() {
        use sui_types::digests::*;
        use sui_types::gas::GasCostSummary;
        use sui_types::messages_checkpoint::*;

        let yaml_content = include_str!("../../sui-core/tests/staged/sui.yaml");
        let parser = Parser::from_yaml(yaml_content).unwrap();

        // Create test CheckpointSummary
        let checkpoint = CheckpointSummary {
            epoch: 1,
            sequence_number: 100,
            network_total_transactions: 1000,
            content_digest: CheckpointContentsDigest::random(),
            previous_digest: Some(CheckpointDigest::random()),
            epoch_rolling_gas_cost_summary: GasCostSummary {
                computation_cost: 100,
                storage_cost: 50,
                storage_rebate: 5,
                non_refundable_storage_fee: 1,
            },
            timestamp_ms: 1234567890,
            checkpoint_commitments: vec![],
            end_of_epoch_data: None,
            version_specific_data: vec![],
        };

        // Round-trip test: encode -> parse -> convert back
        let encoded = bcs::to_bytes(&checkpoint).unwrap();
        let parsed = parser.parse(&encoded, "CheckpointSummary").unwrap();
        let converted_back = CheckpointSummary::try_from(parsed).unwrap();

        assert_eq!(checkpoint, converted_back);
    }

    #[test]
    fn test_gas_cost_summary_round_trip() {
        use std::convert::TryFrom;
        use sui_types::gas::GasCostSummary;

        let yaml_content = include_str!("../../sui-core/tests/staged/sui.yaml");
        let parser = Parser::from_yaml(yaml_content).unwrap();

        // Create test GasCostSummary
        let gas_summary = GasCostSummary {
            computation_cost: 1000,
            storage_cost: 500,
            storage_rebate: 50,
            non_refundable_storage_fee: 10,
        };

        // Round-trip test: encode -> parse -> convert back
        let encoded = bcs::to_bytes(&gas_summary).unwrap();
        let parsed = parser.parse(&encoded, "GasCostSummary").unwrap();
        let converted_back = GasCostSummary::try_from(parsed).unwrap();

        assert_eq!(gas_summary, converted_back);
    }
}
