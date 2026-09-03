import { defineRule } from "@oxlint/plugins"

import { isInsideBoundaryParser } from "../shared/boundary-parsers.ts"

import type { ESTree } from "@oxlint/plugins"

type RuntimeFunction = ESTree.ArrowFunctionExpression | ESTree.Function

function isRuntimeFunction(node: ESTree.Node): node is RuntimeFunction {
  return (
    node.type === "ArrowFunctionExpression" ||
    node.type === "FunctionDeclaration" ||
    node.type === "FunctionExpression"
  )
}

function isInsideTypeGuard(node: ESTree.Node): boolean {
  let current: ESTree.Node | null = node.parent
  while (current !== null && current.type !== "Program") {
    if (isRuntimeFunction(current)) {
      return current.returnType?.typeAnnotation.type === "TSTypePredicate"
    }
    current = current.parent
  }
  return false
}

const defaultOptions = {
  allowInTypeGuards: true,
  allowInBoundaryParsers: true,
} as const

/** Disallow runtime typeof checks that narrow unparsed values instead of decoding them. */
export const noRuntimeTypeofRule = defineRule({
  meta: {
    type: "problem",
    docs: {
      description:
        "Disallow runtime typeof checks; external values must be decoded into meaningful types at their I/O boundary.",
    },
    messages: {
      runtimeTypeof:
        "A `typeof` check narrows a representation without establishing its contract. Parse input at its I/O boundary, then branch on the domain value.",
    },
    schema: [
      {
        type: "object",
        properties: {
          allowInTypeGuards: { type: "boolean" },
          allowInBoundaryParsers: { type: "boolean" },
        },
        additionalProperties: false,
      },
    ],
    defaultOptions: [defaultOptions],
  },
  createOnce(context) {
    return {
      UnaryExpression(node) {
        const option = context.options?.[0]
        const configured =
          typeof option === "object" &&
          option !== null &&
          !Array.isArray(option)
        const allowInTypeGuards = configured
          ? option.allowInTypeGuards === true
          : defaultOptions.allowInTypeGuards
        const allowInBoundaryParsers = configured
          ? option.allowInBoundaryParsers === true
          : defaultOptions.allowInBoundaryParsers
        if (node.operator !== "typeof") return
        if (allowInTypeGuards && isInsideTypeGuard(node)) return
        if (allowInBoundaryParsers && isInsideBoundaryParser(node)) return
        context.report({ node, messageId: "runtimeTypeof" })
      },
    }
  },
})
