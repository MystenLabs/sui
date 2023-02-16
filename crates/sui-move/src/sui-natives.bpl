// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// ==================================================================================
// Native address


procedure {:inline 1} $2_address_from_bytes(bytes: Vec (int)) returns (res: int);

procedure {:inline 1} $2_address_to_u256(addr: int) returns (res: int);

procedure {:inline 1} $2_address_from_u256(num: int) returns (res: int);

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

{%- for instance in dynamic_field_instances %}

{%- set K = instance.0.name -%}
{%- set V = instance.1.name -%}
{%- set S = "'" ~ instance.0.suffix ~ "_" ~ instance.1.suffix ~ "'" -%}
{%- set SK = "'" ~ instance.0.suffix ~ "'" -%}
{%- set SV = "'" ~ instance.1.suffix ~ "'" -%}

// ----------------------------------------------------------------------------------
// Native dynamic field implementation for object type `{{S}}`

procedure {:inline 1} $2_dynamic_field_borrow_mut{{S}}(m: $Mutation ($2_object_UID), k: {{K}})
returns (dst: $Mutation ({{V}}), m': $Mutation ($2_object_UID))
{
    var u: $2_object_UID;
    var e: bool;

    u := $Dereference(m);
    e :=
        $2_dynamic_field_spec_uid_has_field{{SK}}(
            $2_dynamic_field_NameShard{{SK}}_$memory, u, k
        ) &&
        $2_dynamic_field_spec_uid_has_field_with_type{{S}}(
            $2_dynamic_field_PairShard{{S}}_$memory, u, k
        );
    if (!e) {
        call $ExecFailureAbort();
        return;
    }
    // TODO: we cannot talk about the borrowed value here
    m' := m;
}

{%- endfor %}

// ==================================================================================
// Native prover


{%- for instance in prover_vec_instances %}

{%- set S = "'" ~ instance.suffix ~ "'" -%}
{%- set T = instance.name -%}

// ----------------------------------------------------------------------------------
// Native Sui prover implementation for object type `{{instance.suffix}}`

function $2_prover_vec_remove{{S}}(v: Vec ({{T}}), elem_idx: int): Vec ({{T}}) {
    RemoveAtVec(v, elem_idx)
}

{%- endfor %}


// ==================================================================================
// Reads and writes to dynamic fields (skeletons)

function GetDynField<T, V>(o: T, addr: int): V;

function UpdateDynField<T, V>(o: T, addr: int, v: V): T {
    // TODO(mengxu): this is only a partial semantics of the update
    o
}