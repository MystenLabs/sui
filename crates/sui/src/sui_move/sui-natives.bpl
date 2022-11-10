// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// ==================================================================================
// Native transfer


{%- for instance in transfer_instances %}

{%- set S = "'" ~ instance.suffix ~ "'" -%}
{%- set T = instance.name -%}

// ----------------------------------------------------------------------------------
// Native transfer implementation for object type `{{instance.suffix}}`


procedure {:inline 1} $2_transfer_transfer_internal{{S}}(obj: {{T}}, recipient: int, to_object: bool);

procedure {:inline 1} $2_transfer_share_object{{S}}(obj: {{T}});

procedure {:inline 1} $2_transfer_freeze_object{{S}}(obj: {{T}});

{%- endfor %}

// ==================================================================================
// Native object

procedure {:inline 1} $2_object_address_from_bytes(bytes: Vec (int)) returns (res: int);

procedure {:inline 1} $2_object_delete_impl(id: int);

procedure {:inline 1} $2_object_record_new_uid(id: int);

{%- for instance in object_instances %}
{%- set S = "'" ~ instance.suffix ~ "'" -%}
{%- set T = instance.name -%}

// ----------------------------------------------------------------------------------
// Native object implementation for object type `{{instance.suffix}}`

procedure {:inline 1} $2_object_borrow_uid{{S}}(obj: {{T}}) returns (res: $2_object_UID);

{%- endfor %}

// ==================================================================================
// Native tx_context

procedure {:inline 1} $2_tx_context_derive_id(tx_hash: Vec (int), ids_created: int) returns (res: int);

// ==================================================================================
// Native event


{%- for instance in sui_event_instances %}

{%- set S = "'" ~ instance.suffix ~ "'" -%}
{%- set T = instance.name -%}

// ----------------------------------------------------------------------------------
// Native Sui event implementation for object type `{{instance.suffix}}`

procedure {:inline 1} $2_event_emit{{S}}(event: {{T}});

{%- endfor %}

// ==================================================================================
// Native types


{%- for instance in sui_types_instances %}

{%- set S = "'" ~ instance.suffix ~ "'" -%}
{%- set T = instance.name -%}

// ----------------------------------------------------------------------------------
// Native Sui types implementation for object type `{{instance.suffix}}`

procedure {:inline 1} $2_types_is_one_time_witness{{S}}(_: {{T}}) returns (res: bool);

{%- endfor %}

// ==================================================================================
// Native dynamic_field

procedure {:inline 1} $2_dynamic_field_has_child_object(parent: int, id: int) returns (res: bool);

{%- for instance in dynamic_field_instances %}

{%- set S = "'" ~ instance.suffix ~ "'" -%}
{%- set T = instance.name -%}

// ----------------------------------------------------------------------------------
// Native dynamic field implementation for object type `{{instance.suffix}}`

procedure {:inline 1} $2_dynamic_field_hash_type_and_key{{S}}(parent: int, k: {{T}}) returns (res: int);

procedure {:inline 1} $2_dynamic_field_add_child_object{{S}}(parent: int, child: {{T}});

procedure {:inline 1} $2_dynamic_field_borrow_child_object{{S}}(parent: int, id: int): returns (res: {{T}});

procedure {:inline 1} $2_dynamic_field_remove_child_object{{S}}(parent: int, id: int): returns (res: {{T}});

procedure {:inline 1} $2_dynamic_field_has_child_object_with_ty(parent: int, id: int) returns (res: bool);

{%- endfor %}


// ==================================================================================
// Native bls12381

procedure {:inline 1} $2_bls12381_bls12381_min_sig_verify(hash: Vec (int), public_key: Vec (int), msg: Vec (int)) returns (res: bool);

procedure {:inline 1} $2_bls12381_bls12381_min_pk_verify(hash: Vec (int), public_key: Vec (int), msg: Vec (int)) returns (res: bool);

// ==================================================================================
// Native ed25519

procedure {:inline 1} $2_ed25519_ed25519_verify(signature: Vec (int), public_key: Vec (int), msg: Vec (int)) returns (res: bool);

// ==================================================================================
// Native bulletproofs

procedure {:inline 1} $2_bulletproofs_native_verify_full_range_proof(proof: Vec (int), commitment: Vec (int), bit_length: int);

// ==================================================================================
// Native elliptic_curve

procedure {:inline 1} $2_elliptic_curve_native_create_pedersen_commitment(value: Vec (int), blinding_factor: Vec (int)) returns (res: Vec (int));

procedure {:inline 1} $2_elliptic_curve_native_add_ristretto_point(point1: Vec (int), point2: Vec (int)) returns (res: Vec (int));

procedure {:inline 1} $2_elliptic_curve_native_subtract_ristretto_point(point1: Vec (int), point2: Vec (int)) returns (res: Vec (int));

procedure {:inline 1} $2_elliptic_curve_native_scalar_from_u64(value: int) returns (res: Vec (int));

procedure {:inline 1} $2_elliptic_curve_native_scalar_from_bytes(bytes: Vec (int)) returns (res: Vec (int));

// ==================================================================================
// Native hmac

procedure {:inline 1} $2_hmac_native_hmac_sha3_256(key: Vec (int), msg: Vec (int)) returns (res: Vec (int));

procedure {:inline 1} $2_hmac_hmac_sha3_256(key: Vec (int), msg: Vec (int)) returns (res: $2_digest_Sha3256Digest);


