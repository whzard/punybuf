(key_layer (digit) @number) @keyword
(key_include (included) @string) @keyword
(attr) @keyword @attribute
(attr (attr_value_inside) @string)
(doc) @comment.doc
(doc_end "]" @comment.doc) ;; HACK: conflicts with "]" @punctuation.bracket
(doc_nest "[" @comment.doc) ;; HACK: conflicts with "[" @punctuation.bracket
(reference (symbol) @type)
(typedecl (symbol) @type)
(typedecl (generic_params (symbol) @type))
(commanddecl (symbol) @function)
(commanddecl "->" @operator)
":" @punctuation.delimiter
"." @punctuation.delimiter
"," @punctuation.delimiter
"=" @punctuation.delimiter
"?" @operator
"!" @operator
(comment) @comment
"{" @punctuation.bracket
"}" @punctuation.bracket
"<" @punctuation.bracket
">" @punctuation.bracket
"(" @punctuation.bracket
")" @punctuation.bracket
"[" @punctuation.bracket
"]" @punctuation.bracket
(enum_variant (symbol) @variable @property @enum)
(struct_field (symbol) @variable @property)
(flags (symbol) @variable @property)