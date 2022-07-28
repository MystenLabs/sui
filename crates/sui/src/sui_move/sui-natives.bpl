// Copyright (c) 2022, Mysten Labs, Inc.
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

procedure {:inline 1} $2_object_bytes_to_address(bytes: Vec (int)) returns (res: int);

{%- for instance in object_instances %}
{%- set S = "'" ~ instance.suffix ~ "'" -%}
{%- set T = instance.name -%}

// ----------------------------------------------------------------------------------
// Native id implementation for object type `{{instance.suffix}}`


procedure {:inline 1} $2_object_get_info{{S}}(obj: {{T}}) returns (res: $2_object_Info);

procedure {:inline 1} $2_object_delete_impl{{S}}(info: {{T}});

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
