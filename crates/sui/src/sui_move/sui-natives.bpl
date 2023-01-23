// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// ==================================================================================
// Native address


procedure {:inline 1} $2_address_from_bytes(bytes: Vec (int)) returns (res: int);

procedure {:inline 1} $2_address_to_u256(addr: int) returns (res: int);

procedure {:inline 1} $2_address_from_u256(num: int) returns (res: int);

// ==================================================================================
// Native transfer

function {:inline} ownership_update<T>(m: $Memory T, id: int, v: T): $Memory T {
    $Memory(domain#$Memory(m)[id := true], contents#$Memory(m)[id := v])
}

{%- for instance in transfer_instances %}

{%- set S = "'" ~ instance.suffix ~ "'" -%}
{%- set T = instance.name -%}

// ----------------------------------------------------------------------------------
// Native transfer implementation for object type `{{instance.suffix}}`



procedure {:inline 1} $2_transfer_transfer_internal{{S}}(obj: {{T}}, recipient: int) {
    var id: int;
    var v: $2_prover_Ownership;
    id := $bytes#$2_object_ID($2_object_$id{{S}}(obj));
    v := $2_prover_Ownership($1_option_spec_some'address'(recipient), 1);
    $2_prover_Ownership_$memory := ownership_update($2_prover_Ownership_$memory, id, v);
}

procedure {:inline 1} $2_transfer_share_object{{S}}(obj: {{T}}) {
    var id: int;
    var v: $2_prover_Ownership;
    if ($2_prover_owned{{S}}($2_prover_Ownership_$memory, obj)) {
        call $ExecFailureAbort();
        return;
    }

    id := $bytes#$2_object_ID($2_object_$id{{S}}(obj));
    v := $2_prover_Ownership($1_option_Option'address'(EmptyVec()), 2);
    $2_prover_Ownership_$memory := ownership_update($2_prover_Ownership_$memory, id, v);
}

procedure {:inline 1} $2_transfer_freeze_object{{S}}(obj: {{T}}) {
    var id: int;
    var v: $2_prover_Ownership;
    if ($2_prover_owned{{S}}($2_prover_Ownership_$memory, obj)) {
        call $ExecFailureAbort();
        return;
    }

    id := $bytes#$2_object_ID($2_object_$id{{S}}(obj));
    v := $2_prover_Ownership($1_option_Option'address'(EmptyVec()), 3);
    $2_prover_Ownership_$memory := ownership_update($2_prover_Ownership_$memory, id, v);
}

{%- endfor %}

// ==================================================================================
// Native object


procedure {:inline 1} $2_object_delete_impl(id: int);

procedure {:inline 1} $2_object_record_new_uid(id: int);

{%- for instance in object_instances %}
{%- set S = "'" ~ instance.suffix ~ "'" -%}
{%- set T = instance.name -%}

// ----------------------------------------------------------------------------------
// Native object implementation for object type `{{instance.suffix}}`

procedure {:inline 1} $2_object_borrow_uid{{S}}(obj: {{T}}) returns (res: $2_object_UID) {
    res := $id#{{T}}(obj);
}

function $2_object_$borrow_uid{{S}}(obj: {{T}}): $2_object_UID {
    $id#{{T}}(obj)
}


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

{%- for instance_0 in dynamic_field_instances %}
{%- for instance_1 in dynamic_field_instances %}
{%- set S = "'" ~ instance_0.suffix ~ "'" -%}
{%- set T = instance_0.name -%}
{%- set K = "'" ~ instance_0.suffix ~ "_" ~ instance_1.suffix ~ "'" -%}

// ----------------------------------------------------------------------------------
// Native dynamic field implementation for object type `{{instance_0.suffix}}_{{instance_1.suffix}}
// This may be suboptimal as this template will be expanded for all combinations of concrete types
// but handling it probelry would require non-trivial changes to prelude generation code in the core Move repo

procedure {:inline 1} $2_dynamic_field_add{{K}}(parent: $Mutation $2_object_UID, name: {{T}}, value: {{T}}) returns (res: $Mutation $2_object_UID) {
    var id: int;
    var v: $2_prover_DynamicFields{{S}};
    id := $bytes#$2_object_ID($id#$2_object_UID($Dereference(parent)));
    if ($2_prover_uid_has_field{{S}}($2_prover_DynamicFields{{S}}_$memory, id, name)) {
        call $ExecFailureAbort();
        return;
    }
    v := $2_prover_DynamicFields{{S}}(ExtendVec($names#$2_prover_DynamicFields{{S}}($ResourceValue($2_prover_DynamicFields{{S}}_$memory, id)), name));
    $2_prover_DynamicFields{{S}}_$memory := ownership_update($2_prover_DynamicFields{{S}}_$memory, id, v);
    res := parent;
}

{%- endfor %}
{%- endfor %}


{%- for instance in dynamic_field_instances %}
{%- set S = "'" ~ instance.suffix ~ "'" -%}
{%- set T = instance.name -%}

// ----------------------------------------------------------------------------------
// Native dynamic field implementation for object type `{{instance.suffix}}`

procedure {:inline 1} $2_dynamic_field_hash_type_and_key{{S}}(parent: int, k: {{T}}) returns (res: int);

procedure {:inline 1} $2_dynamic_field_add_child_object{{S}}(parent: int, child: {{T}});

procedure {:inline 1} $2_dynamic_field_borrow_child_object{{S}}(object: $2_object_UID, id: int) returns (res: {{T}});

procedure {:inline 1} $2_dynamic_field_borrow_child_object_mut{{S}}(object: $Mutation $2_object_UID, id: int) returns (res: $Mutation ({{T}}), m: $Mutation ($2_object_UID));

procedure {:inline 1} $2_dynamic_field_remove_child_object{{S}}(parent: int, id: int) returns (res: {{T}});

procedure {:inline 1} $2_dynamic_field_has_child_object_with_ty{{S}}(parent: int, id: int) returns (res: bool);

{%- endfor %}

// ==================================================================================
// Reads and writes to dynamic fields (skeletons)

function GetDynField<T, V>(o: T, addr: int): V;

function UpdateDynField<T, V>(o: T, addr: int, v: V): T;
