; Minimal scope tracking for Move.

; Function Scope
(function_definition body: (block) @scope)

(function_parameter name: (variable_identifier) @definition.var)
(function_parameter name: (variable_identifier) @local.definition)

(identifier) @local.reference

; Module Scope
(module_definition
  (module_body) @scope)
