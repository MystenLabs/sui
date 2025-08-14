pub use bcs;

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
    TupleArray { content: Box<TypeLayout>, size: usize },
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
    pub fn from_yaml(yaml_content: &str) -> Result<Self, serde_yaml::Error> {
        let raw_schemas: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(yaml_content)?;
        let mut schemas = HashMap::new();
        
        for (name, value) in raw_schemas {
            if let Ok(layout) = Self::parse_layout(&value) {
                schemas.insert(name, layout);
            }
        }
        
        Ok(Parser { schemas })
    }
    
    fn parse_layout(value: &serde_yaml::Value) -> Result<TypeLayout, String> {
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
                _ => Err(format!("Unknown type: {}", s)),
            },
            Value::Mapping(map) => {
                if let Some(newtype) = map.get(Value::String("NEWTYPESTRUCT".to_string())) {
                    match newtype {
                        Value::String(s) if s == "BYTES" => Ok(TypeLayout::NewtypeStruct(Box::new(TypeLayout::Bytes))),
                        Value::Mapping(inner) => {
                            if let Some(tuple_array) = inner.get(Value::String("TUPLEARRAY".to_string())) {
                                if let Value::Mapping(ta_map) = tuple_array {
                                    let content = ta_map.get(Value::String("CONTENT".to_string()))
                                        .and_then(|v| Self::parse_layout(v).ok())
                                        .ok_or("Missing CONTENT")?;
                                    let size = ta_map.get(Value::String("SIZE".to_string()))
                                        .and_then(|v| v.as_u64())
                                        .ok_or("Missing SIZE")? as usize;
                                    Ok(TypeLayout::NewtypeStruct(Box::new(TypeLayout::TupleArray {
                                        content: Box::new(content),
                                        size,
                                    })))
                                } else {
                                    Err("Invalid TUPLEARRAY".to_string())
                                }
                            } else if let Some(typename) = inner.get(Value::String("TYPENAME".to_string())) {
                                if let Value::String(name) = typename {
                                    Ok(TypeLayout::NewtypeStruct(Box::new(TypeLayout::TypeName(name.clone()))))
                                } else {
                                    Err("Invalid TYPENAME".to_string())
                                }
                            } else {
                                Self::parse_layout(newtype).map(|l| TypeLayout::NewtypeStruct(Box::new(l)))
                            }
                        }
                        _ => Self::parse_layout(newtype).map(|l| TypeLayout::NewtypeStruct(Box::new(l))),
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
                        Err("Invalid STRUCT".to_string())
                    }
                } else if let Some(enum_variants) = map.get(Value::String("ENUM".to_string())) {
                    if let Value::Mapping(variants_map) = enum_variants {
                        let mut parsed_variants = HashMap::new();
                        for (key, val) in variants_map {
                            if let Value::Number(idx) = key {
                                let index = idx.as_u64().ok_or("Invalid enum index")? as u32;
                                if let Value::Mapping(variant_map) = val {
                                    for (name, layout) in variant_map {
                                        if let Value::String(variant_name) = name {
                                            let variant_layout = Self::parse_layout(layout)?;
                                            parsed_variants.insert(index, EnumVariant {
                                                name: variant_name.clone(),
                                                layout: variant_layout,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        Ok(TypeLayout::Enum(parsed_variants))
                    } else {
                        Err("Invalid ENUM".to_string())
                    }
                } else if let Some(seq) = map.get(Value::String("SEQ".to_string())) {
                    Self::parse_layout(seq).map(|l| TypeLayout::Seq(Box::new(l)))
                } else if let Some(tuple) = map.get(Value::String("TUPLE".to_string())) {
                    if let Value::Sequence(elements) = tuple {
                        let mut parsed_elements = Vec::new();
                        for elem in elements {
                            parsed_elements.push(Self::parse_layout(elem)?);
                        }
                        Ok(TypeLayout::Tuple(parsed_elements))
                    } else {
                        Err("Invalid TUPLE".to_string())
                    }
                } else if let Some(typename) = map.get(Value::String("TYPENAME".to_string())) {
                    if let Value::String(name) = typename {
                        Ok(TypeLayout::TypeName(name.clone()))
                    } else {
                        Err("Invalid TYPENAME".to_string())
                    }
                } else {
                    Err("Unknown mapping type".to_string())
                }
            }
            _ => Err("Invalid layout format".to_string()),
        }
    }
    
    pub fn parse(&self, data: &[u8], type_name: &str) -> Result<Value, String> {
        let layout = self.schemas.get(type_name)
            .ok_or_else(|| format!("Type {} not found in schema", type_name))?;
        let mut cursor = std::io::Cursor::new(data);
        self.parse_value(&mut cursor, layout)
    }
    
    fn parse_value(&self, cursor: &mut std::io::Cursor<&[u8]>, layout: &TypeLayout) -> Result<Value, String> {
        use std::io::Read;
        
        match layout {
            TypeLayout::Bool => {
                let mut buf = [0u8; 1];
                cursor.read_exact(&mut buf).map_err(|e| e.to_string())?;
                Ok(Value::Bool(buf[0] != 0))
            }
            TypeLayout::U8 => {
                let mut buf = [0u8; 1];
                cursor.read_exact(&mut buf).map_err(|e| e.to_string())?;
                Ok(Value::U8(buf[0]))
            }
            TypeLayout::U16 => {
                let mut buf = [0u8; 2];
                cursor.read_exact(&mut buf).map_err(|e| e.to_string())?;
                Ok(Value::U16(u16::from_le_bytes(buf)))
            }
            TypeLayout::U32 => {
                let mut buf = [0u8; 4];
                cursor.read_exact(&mut buf).map_err(|e| e.to_string())?;
                Ok(Value::U32(u32::from_le_bytes(buf)))
            }
            TypeLayout::U64 => {
                let mut buf = [0u8; 8];
                cursor.read_exact(&mut buf).map_err(|e| e.to_string())?;
                Ok(Value::U64(u64::from_le_bytes(buf)))
            }
            TypeLayout::U128 => {
                let mut buf = [0u8; 16];
                cursor.read_exact(&mut buf).map_err(|e| e.to_string())?;
                Ok(Value::U128(u128::from_le_bytes(buf)))
            }
            TypeLayout::Bytes => {
                let len = self.read_uleb128(cursor)?;
                let mut buf = vec![0u8; len as usize];
                cursor.read_exact(&mut buf).map_err(|e| e.to_string())?;
                Ok(Value::Bytes(buf))
            }
            TypeLayout::Unit => Ok(Value::Unit),
            TypeLayout::NewtypeStruct(inner) => {
                self.parse_value(cursor, inner)
            }
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
                let variant = variants.get(&(variant_idx as u32))
                    .ok_or_else(|| format!("Unknown variant index: {}", variant_idx))?;
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
            TypeLayout::TypeName(name) => {
                let layout = self.schemas.get(name)
                    .ok_or_else(|| format!("Type {} not found in schema", name))?;
                self.parse_value(cursor, layout)
            }
        }
    }
    
    fn read_uleb128(&self, cursor: &mut std::io::Cursor<&[u8]>) -> Result<u64, String> {
        use std::io::Read;
        
        let mut value = 0u64;
        let mut shift = 0;
        loop {
            let mut buf = [0u8; 1];
            cursor.read_exact(&mut buf).map_err(|e| e.to_string())?;
            let byte = buf[0];
            value |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > 63 {
                return Err("ULEB128 too large".to_string());
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
                    "computationCost" => {
                        if let Value::U64(v) = value {
                            assert_eq!(v, 1000);
                            found_computation = true;
                        }
                    }
                    "storageCost" => {
                        if let Value::U64(v) = value {
                            assert_eq!(v, 2000);
                            found_storage = true;
                        }
                    }
                    "storageRebate" => {
                        if let Value::U64(v) = value {
                            assert_eq!(v, 500);
                            found_rebate = true;
                        }
                    }
                    "nonRefundableStorageFee" => {
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
}