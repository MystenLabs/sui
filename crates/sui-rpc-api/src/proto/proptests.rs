use super::types as proto;
use sui_sdk_types::*;
use test_strategy::proptest;

macro_rules! protobuf_roundtrip_test {
    ($type:ident, $proto:ty) => {
        paste::item! {
            #[proptest]
            #[allow(non_snake_case)]
            fn [< test_protobuf_roundtrip_ $type >] (instance: $type) {
                assert_roundtrip::<$type, $proto>(instance);
            }
        }
    };
}

/// Test that a type `T` can be roundtripped through a protobuf type `P`
fn assert_roundtrip<T, P>(instance: T)
where
    T: PartialEq + std::fmt::Debug + Clone,
    T: for<'a> TryFrom<&'a P, Error: std::fmt::Debug>,
    P: From<T>,
    P: prost::Message + Default,
{
    let proto = P::from(instance.to_owned());

    let proto_bytes = proto.encode_to_vec();

    let deser_from_proto = P::decode(proto_bytes.as_slice()).unwrap();

    let t_from_p = T::try_from(&deser_from_proto).unwrap();

    assert_eq!(instance, t_from_p);
}

protobuf_roundtrip_test!(CheckpointSummary, proto::CheckpointSummary);
protobuf_roundtrip_test!(CheckpointContents, proto::CheckpointContents);
protobuf_roundtrip_test!(Transaction, proto::Transaction);
protobuf_roundtrip_test!(TransactionEffects, proto::TransactionEffects);
protobuf_roundtrip_test!(TransactionEvents, proto::TransactionEvents);
protobuf_roundtrip_test!(Object, proto::Object);
protobuf_roundtrip_test!(UserSignature, proto::UserSignature);
protobuf_roundtrip_test!(
    ValidatorAggregatedSignature,
    proto::ValidatorAggregatedSignature
);
protobuf_roundtrip_test!(ExecutionStatus, proto::ExecutionStatus);
