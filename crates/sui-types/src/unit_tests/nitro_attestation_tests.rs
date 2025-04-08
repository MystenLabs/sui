// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use crate::error::SuiError;
use crate::nitro_attestation::NitroAttestationVerifyError;

use super::{parse_nitro_attestation, verify_nitro_attestation};
use super::{AttestationDocument, CoseSign1};
use ciborium::value::Value;
use fastcrypto::encoding::Encoding;
use fastcrypto::encoding::Hex;

const FIXED_VALID_ATTESTATION: &str = "8444a1013822a0591121a9696d6f64756c655f69647827692d30663733613462346362373463633966322d656e633031393265343138386665663738316466646967657374665348413338346974696d657374616d701b000001932d1239ca6470637273b0005830000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000015830000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000025830000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000035830639a8b65f68b0223cbb14a0032487e5656d260434e3d1a10e7ec1407fb86143860717fc8afee90df7a1604111709af460458309ab5a1aba055ee41ee254b9b251a58259b29fa1096859762744e9ac73b5869b25e51223854d9f86adbb37fe69f3e5d1c0558300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000658300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000758300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000858300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000958300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a58300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b58300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000c58300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000d58300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e58300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000f58300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006b636572746966696361746559027e3082027a30820201a00302010202100192e4188fef781d0000000067366a8d300a06082a8648ce3d04030330818e310b30090603550406130255533113301106035504080c0a57617368696e67746f6e3110300e06035504070c0753656174746c65310f300d060355040a0c06416d617a6f6e310c300a060355040b0c034157533139303706035504030c30692d30663733613462346362373463633966322e75732d656173742d312e6177732e6e6974726f2d656e636c61766573301e170d3234313131343231323432365a170d3234313131353030323432395a308193310b30090603550406130255533113301106035504080c0a57617368696e67746f6e3110300e06035504070c0753656174746c65310f300d060355040a0c06416d617a6f6e310c300a060355040b0c03415753313e303c06035504030c35692d30663733613462346362373463633966322d656e63303139326534313838666566373831642e75732d656173742d312e6177733076301006072a8648ce3d020106052b810400220362000442e0526fc41af71feac64fc6f68a8ac8aae831a9e945ab7d482b842acaf05d6b762d00cbc2115da270187c44597b1c16dcf497c70e543b41612e9041ea143d11d58bd1c847496e5d41ec78a49fe445348cf9a47af9387e0451d9ec145b56ec12a31d301b300c0603551d130101ff04023000300b0603551d0f0404030206c0300a06082a8648ce3d0403030367003064023078001466c0c64293b9bde3d0834edb67ff18417f6075a8f7d137701e10164ce6cf45c508bf383ed0d8d41c51a5977a43023033cb8e4a6ad2686b86c2533accbab5dd5e98cf25d3612b1a48502f327ce00acc921641242d5a3a27d222df1f7dfc3e2c68636162756e646c65845902153082021130820196a003020102021100f93175681b90afe11d46ccb4e4e7f856300a06082a8648ce3d0403033049310b3009060355040613025553310f300d060355040a0c06416d617a6f6e310c300a060355040b0c03415753311b301906035504030c126177732e6e6974726f2d656e636c61766573301e170d3139313032383133323830355a170d3439313032383134323830355a3049310b3009060355040613025553310f300d060355040a0c06416d617a6f6e310c300a060355040b0c03415753311b301906035504030c126177732e6e6974726f2d656e636c617665733076301006072a8648ce3d020106052b8104002203620004fc0254eba608c1f36870e29ada90be46383292736e894bfff672d989444b5051e534a4b1f6dbe3c0bc581a32b7b176070ede12d69a3fea211b66e752cf7dd1dd095f6f1370f4170843d9dc100121e4cf63012809664487c9796284304dc53ff4a3423040300f0603551d130101ff040530030101ff301d0603551d0e041604149025b50dd90547e796c396fa729dcf99a9df4b96300e0603551d0f0101ff040403020186300a06082a8648ce3d0403030369003066023100a37f2f91a1c9bd5ee7b8627c1698d255038e1f0343f95b63a9628c3d39809545a11ebcbf2e3b55d8aeee71b4c3d6adf3023100a2f39b1605b27028a5dd4ba069b5016e65b4fbde8fe0061d6a53197f9cdaf5d943bc61fc2beb03cb6fee8d2302f3dff65902c2308202be30820245a003020102021100ab314210a819b4842e3be045e7daddbe300a06082a8648ce3d0403033049310b3009060355040613025553310f300d060355040a0c06416d617a6f6e310c300a060355040b0c03415753311b301906035504030c126177732e6e6974726f2d656e636c61766573301e170d3234313131333037333235355a170d3234313230333038333235355a3064310b3009060355040613025553310f300d060355040a0c06416d617a6f6e310c300a060355040b0c034157533136303406035504030c2d343834633637303131656563376235332e75732d656173742d312e6177732e6e6974726f2d656e636c617665733076301006072a8648ce3d020106052b8104002203620004cbd3e3fe8793852d952a214ee1c7f17e13eff238c5952ffc6c48f2b8e70beec10194585089829f4818d012a6061cdc9f4d8c5a67aada1233f75b65d3f7704e1c02460cfcc74f0e94193c8d4030f6d1662de0427836c1d32c571c919230fae73aa381d53081d230120603551d130101ff040830060101ff020102301f0603551d230418301680149025b50dd90547e796c396fa729dcf99a9df4b96301d0603551d0e04160414b5f0f617140aa7057c7977f361eee896fd9a58b4300e0603551d0f0101ff040403020186306c0603551d1f046530633061a05fa05d865b687474703a2f2f6177732d6e6974726f2d656e636c617665732d63726c2e73332e616d617a6f6e6177732e636f6d2f63726c2f61623439363063632d376436332d343262642d396539662d3539333338636236376638342e63726c300a06082a8648ce3d04030303670030640230038362cf11e189755d6a2306d728a7f356740eefe623d5e0e9e7c33c1b061ade2224127ac3a2e4bce60b43fc8c53326902306aceccf6f45a8d5c066bd10ce3ffaeeebdee56eedb86deb18ea22172c07196750924dd8f4656c70bd95eb6714cb8ecdd59031a308203163082029ba0030201020211009a0f4f29c1649826edb5b5f9f93b6326300a06082a8648ce3d0403033064310b3009060355040613025553310f300d060355040a0c06416d617a6f6e310c300a060355040b0c034157533136303406035504030c2d343834633637303131656563376235332e75732d656173742d312e6177732e6e6974726f2d656e636c61766573301e170d3234313131343034323230325a170d3234313132303033323230325a308189313c303a06035504030c33373532313933346262636164353432622e7a6f6e616c2e75732d656173742d312e6177732e6e6974726f2d656e636c61766573310c300a060355040b0c03415753310f300d060355040a0c06416d617a6f6e310b3009060355040613025553310b300906035504080c0257413110300e06035504070c0753656174746c653076301006072a8648ce3d020106052b810400220362000496f4565c489625767e8e2d3006ba06bd48ba3e384027a205b93d1ad4958128887c38ddbb2f4922888708ef0985e1e5d3bd73b33f86785ac66a204eed3a6b663686434f64e19fb39cd7b33068edb2108b79774a961e7080cb1b4eaa60a5e63e22a381ea3081e730120603551d130101ff040830060101ff020101301f0603551d23041830168014b5f0f617140aa7057c7977f361eee896fd9a58b4301d0603551d0e0416041484b6dc9994365b56081f5d1bc8ee21f58e45d7df300e0603551d0f0101ff0404030201863081800603551d1f047930773075a073a071866f687474703a2f2f63726c2d75732d656173742d312d6177732d6e6974726f2d656e636c617665732e73332e75732d656173742d312e616d617a6f6e6177732e636f6d2f63726c2f34396230376261342d303533622d346435622d616434612d3364626533653065396637652e63726c300a06082a8648ce3d0403030369003066023100d00c2999e66fbcce624d91aedf41f5532b04c300c86a61d78ed968716a7f7ff565e2c361f4f46fe5c5486a9d2bfe0d60023100bc46872a45820fb552b926d420d4f6a1be831bb26821d374e95bff5ed042b3313465b5b4cde79f16f6a57bd5b541353c5902c3308202bf30820245a003020102021500eaa3f0b662c2a61c96f94194fa33d5baf26eeb84300a06082a8648ce3d040303308189313c303a06035504030c33373532313933346262636164353432622e7a6f6e616c2e75732d656173742d312e6177732e6e6974726f2d656e636c61766573310c300a060355040b0c03415753310f300d060355040a0c06416d617a6f6e310b3009060355040613025553310b300906035504080c0257413110300e06035504070c0753656174746c65301e170d3234313131343130313032345a170d3234313131353130313032345a30818e310b30090603550406130255533113301106035504080c0a57617368696e67746f6e3110300e06035504070c0753656174746c65310f300d060355040a0c06416d617a6f6e310c300a060355040b0c034157533139303706035504030c30692d30663733613462346362373463633966322e75732d656173742d312e6177732e6e6974726f2d656e636c617665733076301006072a8648ce3d020106052b81040022036200040fe46adf864a558a00a9ca4b64ece5ba124ed1d29656a1f16ca71d0dc8fca56b0fb15aafd309f6258374e8c7b4a5b0521c76d1812a7873474dae9322aef1cd782db19fc2ece4d36fa08acbe65e4bec2a3cfe70960d179778ea7e7711f827b36ea366306430120603551d130101ff040830060101ff020100300e0603551d0f0101ff040403020204301d0603551d0e041604143e40d423bf86e9565c378487843389bd2f471a56301f0603551d2304183016801484b6dc9994365b56081f5d1bc8ee21f58e45d7df300a06082a8648ce3d0403030368003065023100c2767f29cc6e40e087617cf680d81e3b77962c29d8ace426b3c4a62a560354da73de6f80986d44da2593a3c268fea94302306056e2f3c88c30170c4940f578acc279a01fe689123e81def4f8c313e1f0cbc44a562a171d12810e847e441aee233f676a7075626c69635f6b6579f669757365725f6461746158205a264748a62368075d34b9494634a3e096e0e48f6647f965b81d2a653de684f2656e6f6e6365f65860284d57f029e1b3beb76455a607b9a86360d6451370f718a0d7bdcad729eea248c25461166ab684ad31fb52713918ee3e401d1b56251d6f9d85bf870e850e0b47559d17091778dbafc3d1989a94bd54c0991053675dcc3686402b189172aae196";
#[test]
fn attestation_parse_and_verify() {
    let parsed = parse_nitro_attestation(&Hex::decode(FIXED_VALID_ATTESTATION).unwrap()).unwrap();

    let res = verify_nitro_attestation(&parsed.0, &parsed.1, &parsed.2, 1731627987382);
    assert!(res.is_ok());

    // cabundle missing one
    let mut mutated_document = parsed.2.clone();
    mutated_document.cabundle.pop();
    let res = verify_nitro_attestation(&parsed.0, &parsed.1, &mutated_document, 1731627987382);
    assert_eq!(
        res.unwrap_err(),
        SuiError::NitroAttestationFailedToVerify(
            "InvalidCertificate: certificate chain issuer mismatch".to_string()
        )
    );

    // corrupted cert
    let mut mutated_document = parsed.2.clone();
    mutated_document.cabundle[parsed.2.cabundle.len() - 1][20] = 0;
    let res = verify_nitro_attestation(&parsed.0, &parsed.1, &mutated_document, 1731627987382);
    assert_eq!(
        res.unwrap_err(),
        SuiError::NitroAttestationFailedToVerify(
            "InvalidCertificate: certificate fails to verify".to_string()
        )
    );

    // corrupted cert
    let mut mutated_document = parsed.2.clone();
    mutated_document.cabundle[0][20] = 0;
    let res = verify_nitro_attestation(&parsed.0, &parsed.1, &mutated_document, 1731627987382);
    assert_eq!(
        res.unwrap_err(),
        SuiError::NitroAttestationFailedToVerify(
            "InvalidCertificate: certificate fails to verify".to_string()
        )
    );
}

#[test]
fn test_over_certificate_expiration() {
    let now = 1731627987382 + 10 * 60 * 1000; // add 10 minute, still valid
    let parsed = parse_nitro_attestation(&Hex::decode(FIXED_VALID_ATTESTATION).unwrap()).unwrap();
    let res = verify_nitro_attestation(&parsed.0, &parsed.1, &parsed.2, now);
    assert!(res.is_ok());

    let now = 1731627987382 - 10 * 60 * 1000; // substract 10 minute, still valid
    let parsed = parse_nitro_attestation(&Hex::decode(FIXED_VALID_ATTESTATION).unwrap()).unwrap();
    let res = verify_nitro_attestation(&parsed.0, &parsed.1, &parsed.2, now);
    assert!(res.is_ok());

    let now = 1731627987382 + 3 * 60 * 60 * 1000; // add 3 hours, cert expired
    let parsed = parse_nitro_attestation(&Hex::decode(FIXED_VALID_ATTESTATION).unwrap()).unwrap();
    let res = verify_nitro_attestation(&parsed.0, &parsed.1, &parsed.2, now);
    assert_eq!(
        res.unwrap_err(),
        SuiError::NitroAttestationFailedToVerify(
            "InvalidCertificate: Certificate timestamp not valid".to_string()
        )
    );

    let now = 1731627987382 - 3 * 60 * 60 * 1000; // subtract 3 hours, cert is not valid yet
    let parsed = parse_nitro_attestation(&Hex::decode(FIXED_VALID_ATTESTATION).unwrap()).unwrap();
    let res = verify_nitro_attestation(&parsed.0, &parsed.1, &parsed.2, now);
    assert_eq!(
        res.unwrap_err(),
        SuiError::NitroAttestationFailedToVerify(
            "InvalidCertificate: Certificate timestamp not valid".to_string()
        )
    );
}

#[test]
fn test_with_malformed_attestation() {
    let err = parse_nitro_attestation(&Hex::decode("0000").unwrap()).unwrap_err();

    assert!(matches!(
        err,
        SuiError::NitroAttestationFailedToVerify(msg) if msg.starts_with("InvalidCoseSign1")
    ));
}

#[test]
fn test_attestation_fields_validity() {
    let mut map = HashMap::new();
    let res = AttestationDocument::validate_document_map(&map);
    assert_eq!(
        res.unwrap_err(),
        NitroAttestationVerifyError::InvalidAttestationDoc("module id not found".to_string())
    );

    // empty module id
    map.insert("module_id".to_string(), Value::Text("".to_string()));
    let res = AttestationDocument::validate_document_map(&map);
    assert_eq!(
        res.unwrap_err(),
        NitroAttestationVerifyError::InvalidAttestationDoc("invalid module id".to_string())
    );
    map.insert("module_id".to_string(), Value::Text("some".to_string()));

    // invalid digest
    map.insert("digest".to_string(), Value::Text("".to_string()));
    let res = AttestationDocument::validate_document_map(&map);
    assert_eq!(
        res.unwrap_err(),
        NitroAttestationVerifyError::InvalidAttestationDoc("invalid digest".to_string())
    );
    map.insert("digest".to_string(), Value::Text("SHA384".to_string()));

    // cert too long, 1025
    map.insert("certificate".to_string(), Value::Bytes(vec![1; 1025]));
    let res = AttestationDocument::validate_document_map(&map);
    assert_eq!(
        res.unwrap_err(),
        NitroAttestationVerifyError::InvalidAttestationDoc("invalid certificate".to_string())
    );
    map.insert("certificate".to_string(), Value::Bytes(vec![1]));

    map.insert(
        "timestamp".to_string(),
        Value::Integer(ciborium::value::Integer::try_from(1731627987382_i128).unwrap()),
    );

    // invalid pcr length
    map.insert(
        "pcrs".to_string(),
        Value::Map(vec![(
            Value::Integer(ciborium::value::Integer::try_from(0_i128).unwrap()),
            Value::Bytes(vec![1; 33]),
        )]),
    );
    let res = AttestationDocument::validate_document_map(&map);
    assert_eq!(
        res.unwrap_err(),
        NitroAttestationVerifyError::InvalidAttestationDoc("invalid PCR value length".to_string())
    );
    map.insert(
        "pcrs".to_string(),
        Value::Map(vec![(
            Value::Integer(ciborium::value::Integer::try_from(0_i128).unwrap()),
            Value::Bytes(vec![1; 32]),
        )]),
    );

    // empty cabundle
    map.insert("cabundle".to_string(), Value::Array(vec![]));
    let res = AttestationDocument::validate_document_map(&map);
    assert_eq!(
        res.unwrap_err(),
        NitroAttestationVerifyError::InvalidAttestationDoc("invalid ca chain length".to_string())
    );
    map.insert(
        "cabundle".to_string(),
        Value::Array(vec![Value::Bytes(vec![1])]),
    );

    // user data too long
    map.insert("user_data".to_string(), Value::Bytes(vec![1; 513]));
    let res = AttestationDocument::validate_document_map(&map);
    assert_eq!(
        res.unwrap_err(),
        NitroAttestationVerifyError::InvalidAttestationDoc("invalid user data".to_string())
    );
    map.insert("user_data".to_string(), Value::Bytes(vec![1; 512]));

    // public key too long
    map.insert("public_key".to_string(), Value::Bytes(vec![1; 1025]));
    let res = AttestationDocument::validate_document_map(&map);
    assert_eq!(
        res.unwrap_err(),
        NitroAttestationVerifyError::InvalidAttestationDoc("invalid public key".to_string())
    );
    map.insert("public_key".to_string(), Value::Bytes(vec![1; 1024]));

    let res = AttestationDocument::validate_document_map(&map);
    assert!(res.is_ok());
}

#[test]
fn bad_signature_cose() {
    let parsed = parse_nitro_attestation(&Hex::decode(FIXED_VALID_ATTESTATION).unwrap()).unwrap();
    let mut bad_sig = parsed.1.clone();
    bad_sig[0] ^= 0x00;
    let res = verify_nitro_attestation(&bad_sig, &parsed.1, &parsed.2, 1731627987382);
    assert_eq!(
        res.unwrap_err(),
        SuiError::NitroAttestationFailedToVerify("InvalidSignature".to_string())
    );
}

#[test]
fn invalid_cose() {
    use crate::nitro_attestation::NitroAttestationVerifyError::InvalidCoseSign1;
    // tests from: https://github.com/awslabs/aws-nitro-enclaves-cose/blob/main/src/sign.rs
    // valid
    let res = CoseSign1::parse_and_validate(&[
        0x84, /* Protected: {1: -35} */
        0x44, 0xA1, 0x01, 0x38, 0x22, /* Unprotected: {4: '11'} */
        0xA1, 0x04, 0x42, 0x31, 0x31, /* payload: */
        0x58, 0x75, 0x49, 0x74, 0x20, 0x69, 0x73, 0x20, 0x61, 0x20, 0x74, 0x72, 0x75, 0x74, 0x68,
        0x20, 0x75, 0x6E, 0x69, 0x76, 0x65, 0x72, 0x73, 0x61, 0x6C, 0x6C, 0x79, 0x20, 0x61, 0x63,
        0x6B, 0x6E, 0x6F, 0x77, 0x6C, 0x65, 0x64, 0x67, 0x65, 0x64, 0x2C, 0x20, 0x74, 0x68, 0x61,
        0x74, 0x20, 0x61, 0x20, 0x73, 0x69, 0x6E, 0x67, 0x6C, 0x65, 0x20, 0x6D, 0x61, 0x6E, 0x20,
        0x69, 0x6E, 0x20, 0x70, 0x6F, 0x73, 0x73, 0x65, 0x73, 0x73, 0x69, 0x6F, 0x6E, 0x20, 0x6F,
        0x66, 0x20, 0x61, 0x20, 0x67, 0x6F, 0x6F, 0x64, 0x20, 0x66, 0x6F, 0x72, 0x74, 0x75, 0x6E,
        0x65, 0x2C, 0x20, 0x6D, 0x75, 0x73, 0x74, 0x20, 0x62, 0x65, 0x20, 0x69, 0x6E, 0x20, 0x77,
        0x61, 0x6E, 0x74, 0x20, 0x6F, 0x66, 0x20, 0x61, 0x20, 0x77, 0x69, 0x66, 0x65,
        0x2E, /* signature - length 48 x 2 */
        0x58, 0x60, /* R: */
        0xCD, 0x42, 0xD2, 0x76, 0x32, 0xD5, 0x41, 0x4E, 0x4B, 0x54, 0x5C, 0x95, 0xFD, 0xE6, 0xE3,
        0x50, 0x5B, 0x93, 0x58, 0x0F, 0x4B, 0x77, 0x31, 0xD1, 0x4A, 0x86, 0x52, 0x31, 0x75, 0x26,
        0x6C, 0xDE, 0xB2, 0x4A, 0xFF, 0x2D, 0xE3, 0x36, 0x4E, 0x9C, 0xEE, 0xE9, 0xF9, 0xF7, 0x95,
        0xA0, 0x15, 0x15, /* S: */
        0x5B, 0xC7, 0x12, 0xAA, 0x28, 0x63, 0xE2, 0xAA, 0xF6, 0x07, 0x8A, 0x81, 0x90, 0x93, 0xFD,
        0xFC, 0x70, 0x59, 0xA3, 0xF1, 0x46, 0x7F, 0x64, 0xEC, 0x7E, 0x22, 0x1F, 0xD1, 0x63, 0xD8,
        0x0B, 0x3B, 0x55, 0x26, 0x25, 0xCF, 0x37, 0x9D, 0x1C, 0xBB, 0x9E, 0x51, 0x38, 0xCC, 0xD0,
        0x7A, 0x19, 0x31,
    ]);
    assert!(res.is_ok());

    // tampered content
    let res = CoseSign1::parse_and_validate(&[
        0x84, /* Protected: {1: -7} */
        0x43, 0xA1, 0x01, 0x26, /* Unprotected: {4: '11'} */
        0xA1, 0x04, 0x42, 0x31, 0x31, /* payload: */
        0x58, 0x75, 0x49, 0x74, 0x20, 0x69, 0x73, 0x20, 0x61, 0x20, 0x74, 0x72, 0x75, 0x74, 0x68,
        0x20, 0x75, 0x6F, 0x69, 0x76, 0x65, 0x72, 0x73, 0x61, 0x6C, 0x6C, 0x79, 0x20, 0x61, 0x63,
        0x6B, 0x6E, 0x6F, 0x77, 0x6C, 0x65, 0x64, 0x67, 0x65, 0x64, 0x2C, 0x20, 0x74, 0x68, 0x61,
        0x74, 0x20, 0x61, 0x20, 0x73, 0x69, 0x6E, 0x67, 0x6C, 0x65, 0x20, 0x6D, 0x61, 0x6E, 0x20,
        0x69, 0x6E, 0x20, 0x70, 0x6F, 0x73, 0x73, 0x65, 0x73, 0x73, 0x69, 0x6F, 0x6E, 0x20, 0x6F,
        0x66, 0x20, 0x61, 0x20, 0x67, 0x6F, 0x6F, 0x64, 0x20, 0x66, 0x6F, 0x72, 0x74, 0x75, 0x6E,
        0x65, 0x2C, 0x20, 0x6D, 0x75, 0x73, 0x74, 0x20, 0x62, 0x65, 0x20, 0x69, 0x6E, 0x20, 0x77,
        0x61, 0x6E, 0x74, 0x20, 0x6F, 0x66, 0x20, 0x61, 0x20, 0x77, 0x69, 0x66, 0x65,
        0x2E, /* Signature - length 32 x 2 */
        0x58, 0x40, /* R: */
        0x6E, 0x6D, 0xF6, 0x54, 0x89, 0xEA, 0x3B, 0x01, 0x88, 0x33, 0xF5, 0xFC, 0x4F, 0x84, 0xF8,
        0x1B, 0x4D, 0x5E, 0xFD, 0x5A, 0x09, 0xD5, 0xC6, 0x2F, 0x2E, 0x92, 0x38, 0x5D, 0xCE, 0x31,
        0xE2, 0xD1, /* S: */
        0x5A, 0x53, 0xA9, 0xF0, 0x75, 0xE8, 0xFB, 0x39, 0x66, 0x9F, 0xCD, 0x4E, 0xB5, 0x22, 0xC8,
        0x5C, 0x92, 0x77, 0x45, 0x2F, 0xA8, 0x57, 0xF5, 0xFE, 0x37, 0x9E, 0xDD, 0xEF, 0x0F, 0xAB,
        0x3C, 0xDD,
    ]);
    assert_eq!(
        res.unwrap_err(),
        InvalidCoseSign1("invalid cbor header".to_string())
    );

    // tampered signature
    let res = CoseSign1::parse_and_validate(&[
        0x84, /* Protected: {1: -7} */
        0x43, 0xA1, 0x01, 0x26, /* Unprotected: {4: '11'} */
        0xA1, 0x04, 0x42, 0x31, 0x31, /* payload: */
        0x58, 0x75, 0x49, 0x74, 0x20, 0x69, 0x73, 0x20, 0x61, 0x20, 0x74, 0x72, 0x75, 0x74, 0x68,
        0x20, 0x75, 0x6E, 0x69, 0x76, 0x65, 0x72, 0x73, 0x61, 0x6C, 0x6C, 0x79, 0x20, 0x61, 0x63,
        0x6B, 0x6E, 0x6F, 0x77, 0x6C, 0x65, 0x64, 0x67, 0x65, 0x64, 0x2C, 0x20, 0x74, 0x68, 0x61,
        0x74, 0x20, 0x61, 0x20, 0x73, 0x69, 0x6E, 0x67, 0x6C, 0x65, 0x20, 0x6D, 0x61, 0x6E, 0x20,
        0x69, 0x6E, 0x20, 0x70, 0x6F, 0x73, 0x73, 0x65, 0x73, 0x73, 0x69, 0x6F, 0x6E, 0x20, 0x6F,
        0x66, 0x20, 0x61, 0x20, 0x67, 0x6F, 0x6F, 0x64, 0x20, 0x66, 0x6F, 0x72, 0x74, 0x75, 0x6E,
        0x65, 0x2C, 0x20, 0x6D, 0x75, 0x73, 0x74, 0x20, 0x62, 0x65, 0x20, 0x69, 0x6E, 0x20, 0x77,
        0x61, 0x6E, 0x74, 0x20, 0x6F, 0x66, 0x20, 0x61, 0x20, 0x77, 0x69, 0x66, 0x65,
        0x2E, /* Signature - length 32 x 2 */
        0x58, 0x40, /* R: */
        0x6E, 0x6D, 0xF6, 0x54, 0x89, 0xEA, 0x3B, 0x01, 0x88, 0x33, 0xF5, 0xFC, 0x4F, 0x84, 0xF8,
        0x1B, 0x4D, 0x5E, 0xFD, 0x5B, 0x09, 0xD5, 0xC6, 0x2F, 0x2E, 0x92, 0x38, 0x5D, 0xCE, 0x31,
        0xE2, 0xD1, /* S: */
        0x5A, 0x53, 0xA9, 0xF0, 0x75, 0xE8, 0xFB, 0x39, 0x66, 0x9F, 0xCD, 0x4E, 0xB5, 0x22, 0xC8,
        0x5C, 0x92, 0x77, 0x45, 0x2F, 0xA8, 0x57, 0xF5, 0xFE, 0x37, 0x9E, 0xDD, 0xEF, 0x0F, 0xAB,
        0x3C, 0xDD,
    ]);
    assert_eq!(
        res.unwrap_err(),
        InvalidCoseSign1("invalid cbor header".to_string())
    );

    // invalid tag
    let res = CoseSign1::parse_and_validate(&[
        0xd3, /* tag 19 */
        0x84, /* Protected: {1: -7} */
        0x43, 0xA1, 0x01, 0x26, /* Unprotected: {4: '11'} */
        0xA1, 0x04, 0x42, 0x31, 0x31, /* payload: */
        0x58, 0x75, 0x49, 0x74, 0x20, 0x69, 0x73, 0x20, 0x61, 0x20, 0x74, 0x72, 0x75, 0x74, 0x68,
        0x20, 0x75, 0x6E, 0x69, 0x76, 0x65, 0x72, 0x73, 0x61, 0x6C, 0x6C, 0x79, 0x20, 0x61, 0x63,
        0x6B, 0x6E, 0x6F, 0x77, 0x6C, 0x65, 0x64, 0x67, 0x65, 0x64, 0x2C, 0x20, 0x74, 0x68, 0x61,
        0x74, 0x20, 0x61, 0x20, 0x73, 0x69, 0x6E, 0x67, 0x6C, 0x65, 0x20, 0x6D, 0x61, 0x6E, 0x20,
        0x69, 0x6E, 0x20, 0x70, 0x6F, 0x73, 0x73, 0x65, 0x73, 0x73, 0x69, 0x6F, 0x6E, 0x20, 0x6F,
        0x66, 0x20, 0x61, 0x20, 0x67, 0x6F, 0x6F, 0x64, 0x20, 0x66, 0x6F, 0x72, 0x74, 0x75, 0x6E,
        0x65, 0x2C, 0x20, 0x6D, 0x75, 0x73, 0x74, 0x20, 0x62, 0x65, 0x20, 0x69, 0x6E, 0x20, 0x77,
        0x61, 0x6E, 0x74, 0x20, 0x6F, 0x66, 0x20, 0x61, 0x20, 0x77, 0x69, 0x66, 0x65,
        0x2E, /* Signature - length 32 x 2 */
        0x58, 0x40, /* R: */
        0x6E, 0x6D, 0xF6, 0x54, 0x89, 0xEA, 0x3B, 0x01, 0x88, 0x33, 0xF5, 0xFC, 0x4F, 0x84, 0xF8,
        0x1B, 0x4D, 0x5E, 0xFD, 0x5A, 0x09, 0xD5, 0xC6, 0x2F, 0x2E, 0x92, 0x38, 0x5D, 0xCE, 0x31,
        0xE2, 0xD1, /* S: */
        0x5A, 0x53, 0xA9, 0xF0, 0x75, 0xE8, 0xFB, 0x39, 0x66, 0x9F, 0xCD, 0x4E, 0xB5, 0x22, 0xC8,
        0x5C, 0x92, 0x77, 0x45, 0x2F, 0xA8, 0x57, 0xF5, 0xFE, 0x37, 0x9E, 0xDD, 0xEF, 0x0F, 0xAB,
        0x3C, 0xDD,
    ]);
    assert_eq!(
        res.unwrap_err(),
        InvalidCoseSign1("invalid tag".to_string())
    );
}
