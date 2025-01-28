; Function Scope
; (function_definition
;   body: (block) @scope
;   parameters: (function_parameters (function_parameter name: (variable_identifier) @definition.var)))

(function_definition body: (block) @scope)

(function_parameter name: (variable_identifier) @definition.var)
(function_parameter name: (variable_identifier) @local.definition)

(identifier) @local.reference


; Module and Struct Scope
(module_definition
  (module_body) @scope)

; (struct_definition
;   (struct_def_fields) @scope)

; Spec Block Scope
(spec_block
  body: (spec_body) @scope)

; Local Variable Declarations in Function Blocks
; (let_statement
;   binds: (bind_list
;            (bind_var (variable_identifier) @definition.var)
;            (bind_unpack 
;              (bind_field (field_identifier) @definition.var))))

; Local Variable Declarations in Spec Blocks
; (spec_block
;   (spec_variable name: (identifier) @definition.var))

; Parameters in Spec Functions
; (spec_function
;   parameters: (function_parameters (function_parameter name: (identifier) @definition.var)))
;
; ; Type Parameters
; (type_parameters
;   (type_parameter name: (identifier) @definition.type)))
;
; ; Struct Fields
; (struct_definition
;   (struct_def_fields
;     (field_annotation
;       field: (identifier) @definition.field)))
;
; ; Function and Module Identifiers
; (function_definition name: (identifier) @definition.function)
; (module_definition name: (identifier) @definition.module)
;
