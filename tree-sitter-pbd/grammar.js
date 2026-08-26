/**
 * @file A definition for the punybuf serialization format
 * @author whezard
 * @license MIT
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

export default grammar({
  name: "pbd",

  extras: $ => [
    /\s+/, /\n/,
    $.comment
  ],
  rules: {
    source: $ => repeat1($.decl),

    decl: $ => choice(
      $.attr,
      $.doc,
      $.key, 
      $.typedecl,
      $.commanddecl
    ),

    attr: $ => seq("@", $.attr_key_string, optional(seq("(", $.attr_value_inside, ")"))),
    attr_key_string: $ => /[^\( \n]+/,
    // attr_value: $ => seq("(", $.attr_value_inside, ")"),
    attr_value_inside: $ => choice($.attr_value_string, seq($.attr_value_string, "(", $.attr_value_inside, ")")),
    attr_value_string: $ => /[^\(\)]+/,

    doc: $ => choice("#[]", seq($.doc_begin, $.doc_inside, $.doc_end)),
    doc_inside: $ => repeat1(choice($.doc_nest, $.one)),
    one: $ => token.immediate(/[^\[\]]+/),
    doc_nest: $ => seq("[", $.doc_inside, $.doc_end),
    doc_begin: $ => "#[",
    doc_end: $ => "]",

    comment: $ => token(seq("#", /[^\[]/, /.*/)),

    digit: $ => /\d/,
    symbol: $ => token(seq(/[\p{L}_]/u, /[\p{L}\p{N}_]*/u)),

    key: $ => choice($.key_include, $.key_layer),
    key_include: $ => seq("include", $.included),
    included: $ => /.*/,
    key_layer: $ => seq("layer", $.digit, ":"),
    typedecl: $ => seq($.symbol, optional($.generic_params), "=", choice($.reference, $.typedef)),
    generic_params: $ => seq(field("open", "<"), repeat(seq($.symbol, optional(","))), field("close", ">")),
    typedef: $ => choice($.struct, $.enum, $.value_enum),

    reference: $ => seq(repeat($.attr), $.symbol, optional($.generic_args), optional($.typedef)),
    generic_args: $ => seq(field("open", "<"), repeat($.reference), field("close", ">")),

    struct: $ => seq(
      field("open", "{"),
      repeat(choice($.doc, prec(2, $.attr), $.struct_field, $.anonymous_flags)),
      field("close", "}")
    ),
    struct_field: $ => seq(
      $.symbol, ":", $.reference,
      optional(seq(".", field("open", "{"), repeat(choice($.doc, $.attr, $.flags)), field("close", "}")))
    ),
    flags: $ => seq($.symbol, "?", optional(seq(":", $.reference))),
    anonymous_flags: $ => seq(
      $.reference, ".",
      field("open", "{"),
      repeat(choice($.doc, $.attr, $.flags)),
      field("close", "}")
    ),

    enum: $ => seq(
      field("open", "["),
      repeat(seq($.enum_variant, ",")),
      optional($.enum_variant),
      field("close", "]")
    ),
    enum_variant: $ => seq(
      repeat(choice($.doc, $.attr)), $.symbol,
      optional(seq(":", $.reference))
    ),

    value_enum: $ => seq(
      field("open", "("),
      repeat(seq(choice($.attr, $.doc, $.reference), ",")),
      optional(choice($.attr, $.doc, $.reference)),
      field("close", ")"),
    ),

    commanddecl: $ => seq($.symbol, ":", $.command_arg, "->", $.reference, optional($.command_error)),
    command_arg: $ => choice($.struct, $.reference, seq(field("open", "("), field("close", ")"))),
    command_error: $ => seq("!", choice($.enum, $.value_enum))
  }
});
