// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// ==================================================================================
// Native address


procedure {:inline 1} $2_address_from_bytes(bytes: Vec (int)) returns (res: int);

procedure {:inline 1} $2_address_to_u256(addr: int) returns (res: int);

procedure {:inline 1} $2_address_from_u256(num: int) returns (res: int);

// ==================================================================================
// Native transfer

{%- for instance in transfer_instances %}

{%- set S = "'" ~ instance.suffix ~ "'" -%}
{%- set T = instance.name -%}

// ----------------------------------------------------------------------------------
// Native transfer implementation for object type `{{instance.suffix}}`


//procedure {:inline 1} $2_transfer_transfer_internal{{S}}(obj: {{T}}, recipient: int);

procedure {:inline 1} $2_transfer_transfer_internal{{S}}(obj: {{T}}, recipient: int) {
    var id: int;
    call id := $2_object_id_address{{S}}(obj);
    {{T}}_$memory := transfer({{T}}_$memory, id, obj, recipient);
}

procedure {:inline 1} $2_transfer_share_object{{S}}(obj: {{T}}) {
    var id: int;
    call id := $2_object_id_address{{S}}(obj);
    if ($2_prover_owned({{T}}_$memory, id)) {
        call $ExecFailureAbort();
        return;
    }
    {{T}}_$memory := share({{T}}_$memory, id, obj);
}

procedure {:inline 1} $2_transfer_freeze_object{{S}}(obj: {{T}});

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
// Reads and writes to dynamic fields (skeletons)

function GetDynField<T, V>(o: T, addr: int): V;

function UpdateDynField<T, V>(o: T, addr: int, v: V): T;

// ==================================================================================
// Spec native functions to be used in specs to tap into Sui storage model


// Representation of object state. The owner value is meaningless if object is shared,
// otherwise itcontains address of the object owner

type {:datatype} $ObjState;
function {:constructor} $ObjState(shared: bool, owner: int): $ObjState;


// Representation of memory for a given type.
type {:datatype} $SuiMemory _;

function {:constructor} $SuiMemory<T>(domain: [int]bool, contents: [int]T, owner: [int]$ObjState): $SuiMemory T;


// Functions to change memory state used in in native function Boogie implementations

function {:inline} transfer<T>(m: $SuiMemory T, id: int, v: T, recipient: int): $SuiMemory T {
    $SuiMemory(domain#$SuiMemory(m)[id := true], contents#$SuiMemory(m)[id := v], owner#$SuiMemory(m)[id := $ObjState(false, recipient)])
}

function {:inline} share<T>(m: $SuiMemory T, id: int, v: T): $SuiMemory T {
    $SuiMemory(domain#$SuiMemory(m)[id := true], contents#$SuiMemory(m)[id := v], owner#$SuiMemory(m)[id := $ObjState(true, 0)])
}

// Functions to query memory state

function {:inline} $2_prover_owned<T>(m: $SuiMemory T, id: int): bool {
    domain#$SuiMemory(m)[id] && !shared#$ObjState(owner#$SuiMemory(m)[id])
}

function {:inline} $2_prover_owned_by<T>(m: $SuiMemory T, id: int, owner: int): bool {
    domain#$SuiMemory(m)[id] && !shared#$ObjState(owner#$SuiMemory(m)[id]) && owner#$ObjState(owner#$SuiMemory(m)[id]) == owner
}

function {:inline} $2_prover_shared<T>(m: $SuiMemory T, id: int): bool {
    domain#$SuiMemory(m)[id] && shared#$ObjState(owner#$SuiMemory(m)[id])
}






