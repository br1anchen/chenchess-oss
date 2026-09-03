import { defineRule } from "@oxlint/plugins"
import type { ESTree } from "@oxlint/plugins"

type TypeAssertionExpression = ESTree.TSAsExpression | ESTree.TSTypeAssertion

function isTypeAssertionExpression(
  node: ESTree.Node,
): node is TypeAssertionExpression {
  return node.type === "TSAsExpression" || node.type === "TSTypeAssertion"
}

function unwrapParenthesizedExpression(
  expression: ESTree.Expression,
): ESTree.Expression {
  let current = expression
  while (current.type === "ParenthesizedExpression") {
    current = current.expression
  }
  return current
}

function isConstAssertion(node: TypeAssertionExpression): boolean {
  const { typeAnnotation } = node
  return (
    typeAnnotation.type === "TSTypeReference" &&
    typeAnnotation.typeName.type === "Identifier" &&
    typeAnnotation.typeName.name === "const"
  )
}

function isOutermostAssertionInChain(node: TypeAssertionExpression): boolean {
  let current: ESTree.Expression = node
  let parent = node.parent

  while (
    parent.type === "ParenthesizedExpression" &&
    parent.expression === current
  ) {
    current = parent
    parent = parent.parent
  }

  return !isTypeAssertionExpression(parent) || parent.expression !== current
}

function isUnknownAssertion(node: TypeAssertionExpression): boolean {
  return node.typeAnnotation.type === "TSUnknownKeyword"
}

function isForbiddenAssertionChain(node: TypeAssertionExpression): boolean {
  const assertions: TypeAssertionExpression[] = []
  let current: ESTree.Expression = node

  while (isTypeAssertionExpression(current)) {
    assertions.push(current)
    current = unwrapParenthesizedExpression(current.expression)
  }

  if (assertions.length <= 1) return false
  if (assertions.every(isConstAssertion)) return false
  const inner = assertions[1]
  // `value as unknown as Owner` widens to the boundary, then names the owner.
  return !(
    assertions.length === 2 &&
    inner !== undefined &&
    isUnknownAssertion(inner)
  )
}

/** Disallow nested TypeScript type assertions, while permitting chains made only of const assertions. */
export const noChainedTypeAssertionsRule = defineRule({
  meta: {
    type: "problem",
    docs: {
      description:
        "Disallow chained TypeScript as and angle-bracket assertions, including parenthesized chains.",
    },
    messages: {
      chained:
        "This assertion chain discards type evidence. Keep the original precise type, or parse untrusted input at its boundary before narrowing it.",
    },
  },
  createOnce(context) {
    const checkTypeAssertion = (node: TypeAssertionExpression) => {
      if (
        !isOutermostAssertionInChain(node) ||
        !isForbiddenAssertionChain(node)
      )
        return
      context.report({ node, messageId: "chained" })
    }

    return {
      TSAsExpression: checkTypeAssertion,
      TSTypeAssertion: checkTypeAssertion,
    }
  },
})
