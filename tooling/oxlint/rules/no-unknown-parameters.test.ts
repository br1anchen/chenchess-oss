import { RuleTester } from "oxlint/plugins-dev"

import { noUnknownParametersRule } from "./no-unknown-parameters.ts"

const tester = new RuleTester({
  languageOptions: { parserOptions: { lang: "ts" } },
})
const error = { messageId: "unknownParameter" }

tester.run("anti-slop/no-unknown-parameters", noUnknownParametersRule, {
  valid: [
    "function parseUser(input: unknown): User { return user; }",
    "const decodePayload = (input: unknown): Payload => payload;",
    "function readConfig(raw: unknown): Config { return config; }",
    "function fromJson(value: unknown): Document { return document; }",
    "function fromNow(value: unknown): Instant { return instant; }",
    "function assertReady(value: unknown): asserts value is Ready {}",
    "function isString(value: unknown): value is string { return true; }",
    "function wrap(cause: unknown): Error { return error; }",
    "function onFailure(error: unknown): void {}",
    "function logErr(err: unknown): void {}",
    "function handle(user: User) {}",
  ],
  invalid: [
    { code: "function handle(input: unknown) {}", errors: [error] },
    { code: "function readyState(value: unknown) {}", errors: [error] },
    { code: "function assertion(value: unknown) {}", errors: [error] },
    { code: "const load = (value: unknown) => value;", errors: [error] },
    { code: "type Loader = (input: unknown) => User;", errors: [error] },
  ],
})
