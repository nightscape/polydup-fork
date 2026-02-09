(function_item
  name: (identifier) @function.name
  body: (block) @function.body) @func

(impl_item
  body: (declaration_list
    (function_item
      name: (identifier) @function.name
      body: (block) @function.body) @func))
