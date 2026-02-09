(function_declaration
  name: (identifier) @function.name
  body: (statement_block) @function.body) @func

(method_definition
  name: (property_identifier) @function.name
  body: (statement_block) @function.body) @func

(arrow_function
  body: (statement_block) @function.body) @func
