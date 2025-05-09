use oxc_allocator::CloneIn;
use oxc_ast::{
    NONE,
    ast::{
        ArrayExpression, ArrayExpressionElement, ArrowFunctionExpression, Expression, Function,
        ObjectExpression, ObjectPropertyKind, TSLiteral, TSMethodSignatureKind, TSTupleElement,
        TSType, TSTypeOperatorOperator,
    },
};
use oxc_span::{GetSpan, SPAN, Span};

use crate::{
    IsolatedDeclarations,
    diagnostics::{
        arrays_with_spread_elements, function_must_have_explicit_return_type,
        inferred_type_of_expression, object_with_spread_assignments, shorthand_property,
    },
    function::get_function_span,
};

impl<'a> IsolatedDeclarations<'a> {
    pub(crate) fn transform_function_to_ts_type(&self, func: &Function<'a>) -> Option<TSType<'a>> {
        let return_type = self.infer_function_return_type(func);
        if return_type.is_none() {
            self.error(function_must_have_explicit_return_type(get_function_span(func)));
        }

        let params = self.transform_formal_parameters(&func.params);

        return_type.map(|return_type| {
            self.ast.ts_type_function_type(
                func.span,
                func.type_parameters.clone_in(self.ast.allocator),
                func.this_param.clone_in(self.ast.allocator),
                params,
                return_type,
            )
        })
    }

    pub(crate) fn transform_arrow_function_to_ts_type(
        &self,
        func: &ArrowFunctionExpression<'a>,
    ) -> Option<TSType<'a>> {
        let return_type = self.infer_arrow_function_return_type(func);

        if return_type.is_none() {
            self.error(function_must_have_explicit_return_type(Span::new(
                func.params.span.start,
                func.body.span.start + 1,
            )));
        }

        let params = self.transform_formal_parameters(&func.params);

        return_type.map(|return_type| {
            self.ast.ts_type_function_type(
                func.span,
                func.type_parameters.clone_in(self.ast.allocator),
                NONE,
                params,
                return_type,
            )
        })
    }

    /// Transform object expression to TypeScript type
    /// ```ts
    /// export const obj = {
    ///  doThing<K extends string>(_k: K): Foo<K> {
    ///    return {} as any;
    ///  },
    /// };
    /// // to
    /// export declare const obj: {
    ///   doThing<K extends string>(_k: K): Foo<K>;
    /// };
    /// ```
    pub fn transform_object_expression_to_ts_type(
        &self,
        expr: &ObjectExpression<'a>,
        is_const: bool,
    ) -> TSType<'a> {
        let members =
            self.ast.vec_from_iter(expr.properties.iter().filter_map(|property| match property {
                ObjectPropertyKind::ObjectProperty(object) => {
                    if object.computed && self.report_property_key(&object.key) {
                        return None;
                    }

                    if object.shorthand {
                        self.error(shorthand_property(object.span));
                        return None;
                    }

                    if let Expression::FunctionExpression(function) = &object.value {
                        if !is_const && object.method {
                            let return_type = self.infer_function_return_type(function);
                            let params = self.transform_formal_parameters(&function.params);
                            return Some(self.ast.ts_signature_method_signature(
                                object.span,
                                object.key.clone_in(self.ast.allocator),
                                object.computed,
                                false,
                                TSMethodSignatureKind::Method,
                                function.type_parameters.clone_in(self.ast.allocator),
                                function.this_param.clone_in(self.ast.allocator),
                                params,
                                return_type,
                            ));
                        }
                    }

                    let type_annotation = if is_const {
                        self.transform_expression_to_ts_type(&object.value)
                    } else {
                        self.infer_type_from_expression(&object.value)
                    };

                    if type_annotation.is_none() {
                        self.error(inferred_type_of_expression(object.value.span()));
                        return None;
                    }

                    let property_signature = self.ast.ts_signature_property_signature(
                        object.span,
                        false,
                        false,
                        is_const,
                        object.key.clone_in(self.ast.allocator),
                        type_annotation.map(|type_annotation| {
                            self.ast.ts_type_annotation(SPAN, type_annotation)
                        }),
                    );
                    Some(property_signature)
                }
                ObjectPropertyKind::SpreadProperty(spread) => {
                    self.error(object_with_spread_assignments(spread.span));
                    None
                }
            }));
        self.ast.ts_type_type_literal(SPAN, members)
    }

    pub(crate) fn transform_array_expression_to_ts_type(
        &self,
        expr: &ArrayExpression<'a>,
        is_const: bool,
    ) -> TSType<'a> {
        let element_types = self.ast.vec_from_iter(expr.elements.iter().filter_map(|element| {
            match element {
                ArrayExpressionElement::SpreadElement(spread) => {
                    self.error(arrays_with_spread_elements(spread.span));
                    None
                }
                ArrayExpressionElement::Elision(elision) => {
                    Some(TSTupleElement::from(self.ast.ts_type_undefined_keyword(elision.span)))
                }
                _ => self
                    .transform_expression_to_ts_type(element.to_expression())
                    .map(TSTupleElement::from)
                    .or_else(|| {
                        self.error(inferred_type_of_expression(element.span()));
                        None
                    }),
            }
        }));

        let ts_type = self.ast.ts_type_tuple_type(SPAN, element_types);
        if is_const {
            self.ast.ts_type_type_operator_type(SPAN, TSTypeOperatorOperator::Readonly, ts_type)
        } else {
            ts_type
        }
    }

    // https://www.typescriptlang.org/docs/handbook/release-notes/typescript-3-4.html#const-assertions
    pub(crate) fn transform_expression_to_ts_type(
        &self,
        expr: &Expression<'a>,
    ) -> Option<TSType<'a>> {
        match expr {
            Expression::BooleanLiteral(lit) => Some(self.ast.ts_type_literal_type(
                SPAN,
                TSLiteral::BooleanLiteral(lit.clone_in(self.ast.allocator)),
            )),
            Expression::NumericLiteral(lit) => Some(self.ast.ts_type_literal_type(
                SPAN,
                TSLiteral::NumericLiteral(lit.clone_in(self.ast.allocator)),
            )),
            Expression::BigIntLiteral(lit) => Some(self.ast.ts_type_literal_type(
                SPAN,
                TSLiteral::BigIntLiteral(lit.clone_in(self.ast.allocator)),
            )),
            Expression::StringLiteral(lit) => Some(self.ast.ts_type_literal_type(
                SPAN,
                TSLiteral::StringLiteral(lit.clone_in(self.ast.allocator)),
            )),
            Expression::NullLiteral(lit) => Some(self.ast.ts_type_null_keyword(lit.span)),
            Expression::Identifier(ident) => match ident.name.as_str() {
                "undefined" => Some(self.ast.ts_type_undefined_keyword(ident.span)),
                _ => None,
            },
            Expression::TemplateLiteral(lit) => {
                self.transform_template_to_string(lit).map(|string| {
                    self.ast.ts_type_literal_type(lit.span, TSLiteral::StringLiteral(string))
                })
            }
            Expression::UnaryExpression(expr) => {
                if Self::can_infer_unary_expression(expr) {
                    Some(self.ast.ts_type_literal_type(
                        SPAN,
                        TSLiteral::UnaryExpression(expr.clone_in(self.ast.allocator)),
                    ))
                } else {
                    None
                }
            }
            Expression::ArrayExpression(expr) => {
                Some(self.transform_array_expression_to_ts_type(expr, true))
            }
            Expression::ObjectExpression(expr) => {
                Some(self.transform_object_expression_to_ts_type(expr, true))
            }
            Expression::FunctionExpression(func) => self.transform_function_to_ts_type(func),
            Expression::ArrowFunctionExpression(func) => {
                self.transform_arrow_function_to_ts_type(func)
            }
            Expression::TSAsExpression(expr) => {
                if expr.type_annotation.is_const_type_reference() {
                    self.transform_expression_to_ts_type(&expr.expression)
                } else {
                    Some(expr.type_annotation.clone_in(self.ast.allocator))
                }
            }
            _ => None,
        }
    }
}
