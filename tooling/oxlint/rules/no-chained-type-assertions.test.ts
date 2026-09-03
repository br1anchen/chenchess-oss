import { RuleTester } from "oxlint/plugins-dev"

import { noChainedTypeAssertionsRule } from "./no-chained-type-assertions.ts"

const tester = new RuleTester({
  languageOptions: { parserOptions: { lang: "ts" } },
})
const error = { messageId: "chained" }

tester.run(
  "anti-slop/no-chained-type-assertions",
  noChainedTypeAssertionsRule,
  {
    valid: [
      "const user = input as User;",
      "const raw = JSON.parse(text) as unknown;",
      "const user = JSON.parse(text) as unknown as User;",
      "const user = <User>(<unknown>JSON.parse(text));",
      "const values = [1] as const;",
    ],
    invalid: [
      { code: "const user = input as object as User;", errors: [error] },
      { code: "const user = input as any as User;", errors: [error] },
      {
        code: "const user = input as unknown as object as User;",
        errors: [error],
      },
    ],
  },
)
