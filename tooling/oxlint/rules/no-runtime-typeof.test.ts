import { RuleTester } from "oxlint/plugins-dev"

import { noRuntimeTypeofRule } from "./no-runtime-typeof.ts"

const tester = new RuleTester({
  languageOptions: { parserOptions: { lang: "ts" } },
})
const error = { messageId: "runtimeTypeof" }
const allowInTypeGuardsOnly = [
  { allowInTypeGuards: true, allowInBoundaryParsers: false },
]
const allowInBoundaryParsersOnly = [
  { allowInTypeGuards: false, allowInBoundaryParsers: true },
]
const denyCarveOuts = [
  { allowInTypeGuards: false, allowInBoundaryParsers: false },
]

tester.run("anti-slop/no-runtime-typeof", noRuntimeTypeofRule, {
  valid: [
    "const value = input;",
    'function isString(value: unknown): value is string { return typeof value === "string"; }',
    'const isString = (value: unknown): value is string => typeof value === "string";',
    'function assertString(value: unknown): asserts value is string { if (typeof value !== "string") throw new Error(); }',
    'function parseName(value: unknown): string { if (typeof value !== "string") throw new Error(); return value; }',
    'const decodeUser = (value: unknown): string => { if (typeof value !== "string") throw new Error(); return value; };',
  ],
  invalid: [
    { code: 'if (typeof input === "string") use(input);', errors: [error] },
    {
      code: 'function isString(value: unknown): value is string { return typeof value === "string"; }',
      options: denyCarveOuts,
      errors: [error],
    },
    {
      code: 'function parse(value: unknown): string { if (typeof value !== "string") throw new Error(); return value; }',
      options: allowInTypeGuardsOnly,
      errors: [error],
    },
    {
      code: 'if (typeof input === "string") use(input);',
      options: allowInBoundaryParsersOnly,
      errors: [error],
    },
    {
      code: 'function isString(value: unknown): value is string { const check = () => typeof value === "string"; return check(); }',
      options: allowInTypeGuardsOnly,
      errors: [error],
    },
  ],
})
