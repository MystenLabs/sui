// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// ==================================================================================
// Native address


const $MAX_ADDRESS: int;
axiom $MAX_ADDRESS == 1461501637330902918203684832716283019655932542975;

const $ADRESS_LENGTH: int;
axiom $ADRESS_LENGTH == 20;

// helper function for converting vector to address.
function $2_peel_vector_to_address(bytes: Vec (int), len: int): int
{
    if len > 0 then 256 * $2_peel_vector_to_address(bytes, len-1) + ReadVec(bytes, len-1)
    else 0
}

axiom (forall v1, v2: Vec (int) :: {$2_peel_vector_to_address(v1, $ADRESS_LENGTH), $2_peel_vector_to_address(v2, $ADRESS_LENGTH)}
   $IsEqual'vec'u8''(v1, v2) <==> $IsEqual'address'($2_peel_vector_to_address(v1, $ADRESS_LENGTH), $2_peel_vector_to_address(v2, $ADRESS_LENGTH)));

axiom (forall v: Vec (int) :: {$2_peel_vector_to_address(v, $ADRESS_LENGTH)}
     ( var r := $2_peel_vector_to_address(v, $ADRESS_LENGTH); $IsValid'address'(r) ));

// procedure that check abort condition, and converting bytes to address.
procedure {:inline 1} $2_address_from_bytes(bytes: Vec (int)) returns (res: int)
{
    var len: int;
    len := LenVec(bytes);
    if (len != $ADRESS_LENGTH) {
        call $ExecFailureAbort();
        return;
    }
    res := $2_peel_vector_to_address(bytes, $ADRESS_LENGTH);
}

function {:inline} $2_address_$from_bytes(bytes: Vec (int)): int {
    $2_peel_vector_to_address(bytes, $ADRESS_LENGTH)
}

function $2_u256_from_address(addr: int): int
{
    addr
}

axiom (forall a1, a2: int :: {$2_u256_from_address(a1), $2_u256_from_address(a2)}
   $IsEqual'address'(a1, a2) <==> $IsEqual'u256'($2_u256_from_address(a1), $2_u256_from_address(a2)));

axiom (forall a: int :: {$2_u256_from_address(a)}
     ( var r := $2_u256_from_address(a); $IsValid'u256'(r) ));

// procedure that check abort condition, and converting address to u256.
procedure {:inline 1} $2_address_to_u256(addr: int) returns (res: int)
{
    if ( !$IsValid'address'(addr) ) {
        call $ExecFailureAbort();
        return;
    }
    res := $2_u256_from_address(addr);
}

function {:inline} $2_address_$to_u256(addr: int): int {
    $2_u256_from_address(addr)
}

function $2_u256_to_address(num: int): int
{
    num
}

axiom (forall n1, n2: int :: {$2_u256_to_address(n1), $2_u256_to_address(n2)}
   $IsEqual'u256'(n1, n2) <==> $IsEqual'address'($2_u256_to_address(n1), $2_u256_to_address(n2)));

axiom (forall n: int :: {$2_u256_to_address(n)}
     ( var r := $2_u256_to_address(n); $IsValid'address'(r) ));

// procedure that check abort condition, and converting u256 to address.
procedure {:inline 1} $2_address_from_u256(num: int) returns (res: int)
{
    if ( !$IsValid'u256'(num) || num > $MAX_ADDRESS ) {
        call $ExecFailureAbort();
        return;
    }
    res := $2_u256_to_address(num);
}

function {:inline} $2_address_$from_u256(num: int): int {
    $2_u256_to_address(num)
}

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
// Native prover


{%- for instance in prover_instances %}

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

function UpdateDynField<T, V>(o: T, addr: int, v: V): T;
