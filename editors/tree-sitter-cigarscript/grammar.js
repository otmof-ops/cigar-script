// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

// A tree-sitter grammar for CigarScript (.cigar) and CigScript (.cig), for
// highlighting, folding and structural editing in Neovim, Helix and Zed.
//
// It is the grammar of docs/LANGUAGE.md (every strength at once: the editor
// does not know the strength a file will be checked at) over CigScript's own
// statements. It is a superset on purpose: a newline ends a statement in the
// language, but here newlines are whitespace, so a tree is produced for a file
// the checker would refuse. `cigar check` and `cigar-lsp` judge; this only
// reads. Known limit: a line that starts `into for T = f` right after a line
// that ends in an expression reads as `expr into for...` (newlines are not
// separators here); put layer meanings after a declaration of another kind, or
// after a `;`. `cigar` words that are contextual in the language (`match`, `use`,
// `record`...) are keywords only where their syntax begins.

const PREC = {
  pipe: 2,
  or: 3,
  and: 4,
  not: 5,
  coalesce: 6,
  compare: 7,
  range: 8,
  add: 9,
  mul: 10,
  unary: 11,
  postfix: 12,
};

/** @param {RuleOrLiteral} rule */
const commaSep = (rule) => optional(commaSep1(rule));
/** @param {RuleOrLiteral} rule */
const commaSep1 = (rule) => seq(rule, repeat(seq(',', rule)), optional(','));

/** @param {RuleOrLiteral} rule */
const list1 = (rule) => seq(rule, repeat(prec.right(seq(',', rule))));

module.exports = grammar({
  name: 'cigarscript',

  extras: ($) => [/[\s\u00a0]/, $.comment],

  word: ($) => $.identifier,

  supertypes: ($) => [$._statement, $._expression, $._type, $._pattern],

  conflicts: ($) => [
    [$._expression, $.record_literal],
    [$.measure_for, $._expression],
    [$.block, $.map_literal],
  ],

  rules: {
    source_file: ($) => seq(optional($.strength_declaration), repeat($._statement)),

    comment: (_) => token(seq('#', /[^\n]*/)),

    strength_declaration: (_) =>
      prec(2, seq('strength', field('level', choice('mild', 'medium', 'full')))),

    // ---- statements -------------------------------------------------------

    _statement: ($) =>
      choice(
        $.declaration,
        $.pull_declaration,
        $.record_declaration,
        $.enum_declaration,
        $.interface_declaration,
        $.impl_declaration,
        $.use_declaration,
        $.layer_declaration,
        $.measure_for,
        $.chain_declaration,
        $.scope_declaration,
        $.if_statement,
        $.while_statement,
        $.for_statement,
        $.parallel_for,
        $.within_statement,
        $.try_statement,
        $.burn_statement,
        $.return_statement,
        $.exhale_statement,
        $.cough_statement,
        $.break_statement,
        $.continue_statement,
        $.assignment,
        $.expression_statement,
        ';',
      ),

    block: ($) => seq('{', repeat($._statement), '}'),

    declaration: ($) =>
      seq(
        optional('pub'),
        choice('stick', 'roll'),
        field('name', $._name),
        optional(seq(':', field('type', $._type))),
        '=',
        field('value', $._expression),
      ),

    type_parameters: ($) => seq('<', commaSep1($.identifier), '>'),

    parameters: ($) => seq('(', commaSep($.parameter), ')'),

    parameter: ($) =>
      seq(
        optional(choice('owned', 'borrow')),
        field('name', $._name),
        optional(seq(':', field('type', $._type))),
      ),

    pull_declaration: ($) =>
      seq(
        optional('pub'),
        'pull',
        field('name', $._name),
        optional($.type_parameters),
        $.parameters,
        optional(seq('->', field('return_type', $._type))),
        optional(seq('coughs', list1($.identifier))),
        field('body', $.block),
      ),

    record_declaration: ($) =>
      seq(
        optional('pub'),
        'record',
        field('name', $.identifier),
        optional($.type_parameters),
        '{',
        commaSep($.field_declaration),
        '}',
      ),

    field_declaration: ($) => seq(field('name', $._name), ':', field('type', $._type)),

    enum_declaration: ($) =>
      seq(
        optional('pub'),
        'enum',
        field('name', $.identifier),
        optional($.type_parameters),
        '{',
        commaSep($.variant),
        '}',
      ),

    variant: ($) =>
      seq(
        field('name', $.identifier),
        optional(seq('(', commaSep(choice($.field_declaration, $._type)), ')')),
      ),

    interface_declaration: ($) =>
      seq(
        optional('pub'),
        'interface',
        field('name', $.identifier),
        optional($.type_parameters),
        '{',
        repeat($.method_signature),
        '}',
      ),

    method_signature: ($) =>
      seq(
        'pull',
        field('name', $._name),
        '(',
        commaSep(choice('self', $.parameter)),
        ')',
        optional(seq('->', field('return_type', $._type))),
      ),

    impl_declaration: ($) =>
      seq(
        'impl',
        field('interface', $.identifier),
        'for',
        field('type', $._type),
        '{',
        repeat($.pull_declaration),
        '}',
      ),

    // `use tools.plan` and `use tools.plan.{Move}`
    use_declaration: ($) =>
      seq(
        'use',
        field('path', $.module_path),
      ),

    module_path: ($) =>
      seq($.identifier, repeat(seq('.', choice($.identifier, $.import_list)))),

    import_list: ($) => seq('{', commaSep1($.identifier), '}'),

    layer_declaration: ($) => seq('layer', field('name', $.identifier)),

    // `measure for T = f`, `into for T = f`, `name for T = f`
    measure_for: ($) =>
      seq(
        field('layer', choice('measure', 'into', $.identifier)),
        'for',
        field('type', $._type),
        '=',
        field('value', $._expression),
      ),

    chain_declaration: ($) =>
      seq('chain', field('name', $.identifier), '{', commaSep($._expression), '}'),

    scope_declaration: ($) => seq('pack', '{', commaSep($._expression), '}'),

    if_statement: ($) =>
      prec.right(
        seq(
          'if',
          field('condition', $._expression),
          field('consequence', $.block),
          optional(seq('else', field('alternative', choice($.if_statement, $.block)))),
        ),
      ),

    while_statement: ($) =>
      seq('while', field('condition', $._expression), field('body', $.block)),

    for_statement: ($) =>
      seq(
        'for',
        field('variable', $._name),
        'in',
        field('iterable', $._expression),
        field('body', $.block),
      ),

    parallel_options: ($) =>
      seq('parallel', '(', 'threads', ':', field('threads', $._expression), ')'),

    parallel_for: ($) =>
      seq(
        $.parallel_options,
        'for',
        field('variable', $._name),
        'in',
        field('iterable', $._expression),
        field('body', $.block),
      ),

    // `stick total = parallel(threads: 4) for n in xs reduce(+) { ... }`
    parallel_value: ($) =>
      seq(
        $.parallel_options,
        'for',
        field('variable', $._name),
        'in',
        field('iterable', $._expression),
        choice('collect', seq('reduce', '(', choice('+', '*', 'min', 'max'), ')')),
        field('body', $.block),
      ),

    within_statement: ($) => seq('within', $.context, field('body', $.block)),

    context: ($) =>
      choice(
        seq('retry', '(', $._expression, optional(seq(',', 'backoff', ':', $._expression)), ')'),
        $.parallel_options,
        seq('timeout', '(', optional($._expression), ')'),
      ),

    try_statement: ($) =>
      seq(
        'try',
        field('body', $.block),
        optional(seq('ashtray', optional(field('error', $.identifier)), field('handler', $.block))),
        optional(seq('finally', field('cleanup', $.block))),
      ),

    burn_statement: ($) =>
      seq(
        'burn',
        optional('unlit'),
        optional(seq('(', $._expression, optional(seq(',', $._expression)), ')')),
        field('body', $.block),
        optional(seq('unburn', field('compensation', $.block))),
      ),

    return_statement: ($) => prec.right(seq('snuff', optional(list1($._expression)))),
    exhale_statement: ($) => prec.right(seq('exhale', list1($._expression))),
    cough_statement: ($) => seq('cough', $._expression),
    break_statement: (_) => 'break',
    continue_statement: (_) => 'continue',

    assignment: ($) =>
      prec(
        0,
        seq(
          field('left', $._expression),
          field('operator', choice('=', '+=', '-=', '*=', '/=')),
          field('right', $._expression),
        ),
      ),

    expression_statement: ($) => $._expression,

    // ---- types ------------------------------------------------------------

    _type: ($) =>
      choice($.named_type, $.optional_type, $.function_type, $.sized_type),

    named_type: ($) => seq($.identifier, optional($.type_arguments)),
    sized_type: (_) => token(choice('u8', 'u16', 'u32', 'u64', 'i8', 'i16', 'i32', 'i64')),
    type_arguments: ($) => seq('<', commaSep1($._type), '>'),
    optional_type: ($) => prec(1, seq($._type, '?')),
    function_type: ($) =>
      prec.right(seq('(', commaSep($._type), ')', '->', field('return_type', $._type))),

    // ---- expressions ------------------------------------------------------

    _expression: ($) =>
      choice(
        $.binary_expression,
        $.unary_expression,
        $.call_expression,
        $.member_expression,
        $.index_expression,
        $.try_expression,
        $.parenthesized_expression,
        $.lambda,
        $.match_expression,
        $.measure_expression,
        $.move_expression,
        $.channel_expression,
        $.parallel_value,
        $.record_literal,
        $.map_literal,
        $.list_literal,
        $.string,
        $.raw_string,
        $.duration,
        $.float,
        $.integer,
        $.true,
        $.false,
        $.null,
        $.raw_identifier,
        $.identifier,
      ),

    binary_expression: ($) =>
      choice(
        ...[
          ['into', PREC.pipe - 1],
          ['>>', PREC.pipe],
          ['or', PREC.or],
          ['and', PREC.and],
          ['??', PREC.coalesce],
          ['==', PREC.compare],
          ['!=', PREC.compare],
          ['<', PREC.compare],
          ['<=', PREC.compare],
          ['>', PREC.compare],
          ['>=', PREC.compare],
          ['..', PREC.range],
          ['+', PREC.add],
          ['-', PREC.add],
          ['*', PREC.mul],
          ['/', PREC.mul],
          ['%', PREC.mul],
        ].map(([op, p]) =>
          prec.left(
            Number(p),
            seq(field('left', $._expression), field('operator', String(op)), field('right', $._expression)),
          ),
        ),
      ),

    unary_expression: ($) =>
      choice(
        prec(PREC.unary, seq(field('operator', '-'), field('argument', $._expression))),
        prec(PREC.not, seq(field('operator', 'not'), field('argument', $._expression))),
      ),

    call_expression: ($) =>
      prec(
        PREC.postfix,
        seq(
          field('function', $._expression),
          field('arguments', $.arguments),
        ),
      ),

    arguments: ($) => seq('(', commaSep(choice($.named_argument, $._expression)), ')'),

    named_argument: ($) =>
      prec(1, seq(field('name', $._name), ':', field('value', $._expression))),

    member_expression: ($) =>
      prec(
        PREC.postfix,
        seq(
          field('object', $._expression),
          field('operator', choice('.', '?.')),
          field('property', $._name),
        ),
      ),

    index_expression: ($) =>
      prec(PREC.postfix, seq(field('object', $._expression), '[', field('index', $._expression), ']')),

    // `parse(x)?`: the value, or return the error
    try_expression: ($) => prec(PREC.postfix, seq($._expression, '?')),

    parenthesized_expression: ($) => seq('(', $._expression, ')'),

    lambda: ($) =>
      prec.right(
        seq(
          'pack',
          $.parameters,
          choice(seq('=>', field('body', $._expression)), field('body', $.block)),
        ),
      ),

    measure_expression: ($) => prec(PREC.unary, seq('measure', $._expression)),
    move_expression: ($) => prec(PREC.unary, seq('move', $._expression)),
    channel_expression: ($) => seq('channel', '<', $._type, '>', '(', ')'),

    match_expression: ($) =>
      seq('match', field('subject', $._expression), '{', repeat($.match_arm), '}'),

    match_arm: ($) =>
      seq(
        field('pattern', $._pattern),
        optional(seq('if', field('guard', $._expression))),
        '=>',
        field('body', choice($._expression, $.block, $.exhale_statement, $.return_statement,
          $.cough_statement, $.break_statement, $.continue_statement, $.assignment)),
        optional(','),
      ),

    _pattern: ($) =>
      choice($.wildcard, $.string, $.raw_string, $.float, $.integer, $.true, $.false, $.null,
        $.variant_pattern, $.record_pattern, $.identifier),

    wildcard: (_) => '_',

    variant_pattern: ($) =>
      seq(
        optional(seq(field('enum', $.identifier), '.')),
        field('variant', $.identifier),
        '(',
        commaSep($._pattern),
        ')',
      ),

    record_pattern: ($) =>
      seq(field('record', $.identifier), '{', commaSep($.field_pattern), '}'),

    field_pattern: ($) => seq(field('name', $.identifier), optional(seq(':', $._pattern))),

    record_literal: ($) =>
      prec.dynamic(1, seq(field('type', $.identifier), '{', commaSep1($.record_field), '}')),

    record_field: ($) => seq(field('name', $._name), ':', field('value', $._expression)),

    map_literal: ($) => seq('{', commaSep($.map_pair), '}'),

    map_pair: ($) =>
      seq(
        field('key', choice($._name, $.string, $.raw_string, seq('[', $._expression, ']'))),
        ':',
        field('value', $._expression),
      ),

    list_literal: ($) => seq('[', commaSep($._expression), ']'),

    // ---- names and literals ----------------------------------------------

    // Words that are keywords at some strength are still names after `.` and
    // as keys, so a name there may be any word.
    _name: ($) => choice($.raw_identifier, $.identifier),

    identifier: (_) => /[A-Za-z_][A-Za-z0-9_]*/,

    // `r#match`: the name `match`, though `match` is reserved.
    raw_identifier: (_) => token(prec(1, /r#[A-Za-z_][A-Za-z0-9_]*/)),

    integer: (_) => token(choice(/\d[\d_]*/, /0[xX][0-9a-fA-F_]+/, /0[bB][01_]+/)),
    float: (_) => token(choice(/\d[\d_]*\.\d[\d_]*([eE][+-]?\d+)?/, /\d[\d_]*[eE][+-]?\d+/)),
    // `500ms`, `2s`, `1m` (a `within timeout` duration)
    duration: (_) => token(prec(1, /\d+(ms|s|m|h)/)),

    true: (_) => 'true',
    false: (_) => 'false',
    null: (_) => 'null',

    string: ($) =>
      seq(
        '"',
        repeat(choice($.string_content, $.escape_sequence, $.interpolation)),
        '"',
      ),
    string_content: ($) => choice($._string_text, alias($._string_dollar, $.string_content)),
    _string_text: (_) => token.immediate(prec(1, /[^"\\$]+/)),
    // a `$` that does not start `${`
    _string_dollar: (_) => token.immediate(prec(-1, '$')),
    escape_sequence: (_) =>
      token.immediate(seq('\\', choice(/[ntr0\\"$]/, /u\{[0-9a-fA-F]+\}/))),
    interpolation: ($) => seq(token.immediate(prec(2, '${')), $._expression, '}'),

    raw_string: (_) => token(seq("'", /[^']*/, "'")),
  },
});
