import type { ESTree } from "@oxlint/plugins"

type NamedOwner =
  | ESTree.ArrowFunctionExpression
  | ESTree.Function
  | ESTree.TSCallSignatureDeclaration
  | ESTree.TSConstructSignatureDeclaration
  | ESTree.TSConstructorType
  | ESTree.TSFunctionType
  | ESTree.TSMethodSignature

// Exact verb or camelCase continuation (`parseUser`). Rejects `readyState`
// and `assertion`. `fromNow` still matches `from` + `Now`.
const BOUNDARY_NAME = /^(?:parse|decode|assert|read|from)(?:$|[A-Z])/u

function isRuntimeFunction(
  node: ESTree.Node,
): node is ESTree.ArrowFunctionExpression | ESTree.Function {
  return (
    node.type === "ArrowFunctionExpression" ||
    node.type === "FunctionDeclaration" ||
    node.type === "FunctionExpression"
  )
}

function identifierName(node: ESTree.Node): string | null {
  return node.type === "Identifier" && typeof node.name === "string"
    ? node.name
    : null
}

/** Function or alias names that own a parse/decode boundary. */
export function isBoundaryParserName(name: string): boolean {
  return BOUNDARY_NAME.test(name)
}

/** Resolve the declared name of a function-like node when it has one. */
export function ownerName(node: NamedOwner): string | null {
  if (
    node.type === "FunctionDeclaration" ||
    node.type === "TSDeclareFunction" ||
    node.type === "FunctionExpression"
  ) {
    return node.id?.name ?? parentBoundName(node)
  }
  if (node.type === "TSMethodSignature") {
    return identifierName(node.key) ?? parentBoundName(node)
  }
  return parentBoundName(node)
}

function parentBoundName(node: ESTree.Node): string | null {
  const parent = node.parent
  if (parent === null || parent === undefined) return null
  if (
    parent.type === "VariableDeclarator" &&
    parent.id.type === "Identifier" &&
    parent.init === node
  ) {
    return parent.id.name
  }
  if (
    (parent.type === "Property" || parent.type === "PropertyDefinition") &&
    parent.value === node
  ) {
    return identifierName(parent.key)
  }
  if (parent.type === "MethodDefinition" && parent.value === node) {
    return identifierName(parent.key)
  }
  if (
    parent.type === "TSTypeAliasDeclaration" &&
    parent.typeAnnotation === node
  ) {
    return parent.id.name
  }
  return null
}

function hasTypePredicateReturn(node: NamedOwner | ESTree.Node): boolean {
  if (!("returnType" in node)) return false
  const returnType = node.returnType
  return returnType?.typeAnnotation.type === "TSTypePredicate"
}

/** True when this function-like node is a named boundary parser or type predicate. */
export function isBoundaryParser(node: NamedOwner): boolean {
  if (hasTypePredicateReturn(node)) return true
  const name = ownerName(node)
  return name !== null && isBoundaryParserName(name)
}

/** Walk parents to find a boundary parser or type-predicate function. */
export function isInsideBoundaryParser(node: ESTree.Node): boolean {
  let current: ESTree.Node | null = node.parent
  while (current !== null && current.type !== "Program") {
    if (isRuntimeFunction(current) && isBoundaryParser(current)) return true
    current = current.parent
  }
  return false
}
