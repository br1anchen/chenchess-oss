/**
 * Writes the RSA key pair Coach Engine's tests sign and verify with.
 *
 * The pair used to be checked in. It is generated instead because a committed
 * file opening with a PEM private-key header is a supported secret-scanning
 * pattern, and push protection refuses the push before it lands — a README
 * calling the key synthetic does not reach the scanner. Generating it is also
 * simply more honest: nothing about these tests needs one particular key, only
 * a real one.
 *
 * Idempotent. An existing pair is left alone, because Cargo would otherwise
 * rebuild every test that reads it on each run.
 */
import { generateKeyPairSync } from "node:crypto";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

/** The `kid` `routes/tests/firebase_token.rs` signs with. */
const KEY_ID = "firebase-test-key";

const FIXTURES = resolve(
  import.meta.dirname,
  "../../services/coach-engine/certification-fixtures",
);
const PRIVATE_KEY = resolve(FIXTURES, "auth-private-key.pem");
const JWKS = resolve(FIXTURES, "auth-jwks.json");

if (existsSync(PRIVATE_KEY) && existsSync(JWKS)) {
  process.stdout.write("Coach Engine test keys are already generated\n");
} else {
  const { privateKey, publicKey } = generateKeyPairSync("rsa", {
    modulusLength: 2048,
  });
  const { n, e } = publicKey.export({ format: "jwk" });
  mkdirSync(FIXTURES, { recursive: true });
  writeFileSync(
    PRIVATE_KEY,
    privateKey.export({ format: "pem", type: "pkcs8" }),
    { mode: 0o600 },
  );
  writeFileSync(
    JWKS,
    `${JSON.stringify(
      { keys: [{ alg: "RS256", e, kid: KEY_ID, kty: "RSA", n, use: "sig" }] },
      null,
      2,
    )}\n`,
  );
  process.stdout.write(`generated Coach Engine test keys in ${FIXTURES}\n`);
}
