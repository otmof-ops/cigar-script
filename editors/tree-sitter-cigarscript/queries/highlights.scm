; SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
; SPDX-License-Identifier: FSL-1.1-ALv2

(comment) @comment

; ---- literals
(string) @string
(raw_string) @string
(string_content) @string
(escape_sequence) @string.escape
(interpolation ["${" "}"] @punctuation.special)
(integer) @number
(float) @number
(duration) @number
[(true) (false)] @boolean
(null) @constant.builtin

; ---- names
(identifier) @variable
(raw_identifier) @variable
(parameter name: (identifier) @variable.parameter)
(field_declaration name: (identifier) @property)
(record_field name: (identifier) @property)
(map_pair key: (identifier) @property)
(member_expression property: (identifier) @property)
(member_expression property: (raw_identifier) @property)
(named_argument name: (identifier) @variable.parameter)
(field_pattern name: (identifier) @property)

(declaration name: (identifier) @variable)
(pull_declaration name: (identifier) @function)
(method_signature name: (identifier) @function)
(call_expression function: (identifier) @function.call)
(call_expression function: (member_expression property: (identifier) @function.method.call))
(lambda "pack" @keyword.function)

; ---- types
(named_type (identifier) @type)
(sized_type) @type.builtin
(record_declaration name: (identifier) @type)
(enum_declaration name: (identifier) @type)
(interface_declaration name: (identifier) @type)
(impl_declaration interface: (identifier) @type)
(type_parameters (identifier) @type)
(record_literal type: (identifier) @type)
(record_pattern record: (identifier) @type)
(variant name: (identifier) @constructor)
(variant_pattern variant: (identifier) @constructor)
(variant_pattern enum: (identifier) @type)
(module_path (identifier) @module)
(layer_declaration name: (identifier) @type)
(wildcard) @variable.builtin

; ---- keywords
[
  "stick" "roll" "pull" "snuff" "exhale" "cough" "try" "ashtray" "finally"
  "burn" "unlit" "unburn" "chain" "pack"
] @keyword

[ "if" "else" "match" ] @keyword.conditional
[ "while" "for" "in" (break_statement) (continue_statement) ] @keyword.repeat

; contextual: medium and full
[
  "strength" "record" "enum" "interface" "impl" "use" "pub" "coughs"
  "into" "measure" "owned" "borrow" "move" "parallel" "channel" "within"
  "layer" "threads" "retry" "backoff" "timeout" "collect" "reduce"
] @keyword

(strength_declaration level: _ @constant.builtin)

; ---- operators and punctuation
[
  "=" "+=" "-=" "*=" "/=" "+" "-" "*" "/" "%" "==" "!=" "<" "<=" ">" ">="
  "??" "?." ">>" ".." "=>" "->" "and" "or" "not" "?"
] @operator

["(" ")" "[" "]" "{" "}"] @punctuation.bracket
["," ";" ":" "."] @punctuation.delimiter
