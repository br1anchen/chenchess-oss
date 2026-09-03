import assert from "node:assert/strict"

import { isBoundaryParserName } from "./boundary-parsers.ts"

assert.equal(isBoundaryParserName("parse"), true)
assert.equal(isBoundaryParserName("parseUser"), true)
assert.equal(isBoundaryParserName("decodePayload"), true)
assert.equal(isBoundaryParserName("assertReady"), true)
assert.equal(isBoundaryParserName("readConfig"), true)
assert.equal(isBoundaryParserName("fromJson"), true)
assert.equal(isBoundaryParserName("readyState"), false)
assert.equal(isBoundaryParserName("assertion"), false)
// `from` + `Now` is still a camelCase collision with `fromJson`.
assert.equal(isBoundaryParserName("fromNow"), true)
