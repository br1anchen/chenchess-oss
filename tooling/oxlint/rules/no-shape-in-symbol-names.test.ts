import { RuleTester } from "oxlint/plugins-dev"

import { noForbiddenTermInSymbolNamesRule } from "./no-shape-in-symbol-names.ts"

const tester = new RuleTester({
  languageOptions: { parserOptions: { lang: "ts" } },
})
const error = { messageId: "forbiddenSymbolName" }

tester.run(
  "anti-slop/no-shape-in-symbol-names",
  noForbiddenTermInSymbolNamesRule,
  {
    valid: [
      "const payload = value;",
      "function parseReject(reason: string) {}",
      "type OwnerContract = { readonly id: string };",
    ],
    invalid: [
      { code: "const shape = value;", errors: [error] },
      { code: "function parseShape(value: unknown) {}", errors: [error] },
      { code: "type PayloadShape = { readonly id: string };", errors: [error] },
    ],
  },
)
