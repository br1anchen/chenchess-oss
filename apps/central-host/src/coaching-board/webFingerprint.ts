/**
 * Deterministic charset-safe fingerprint for web-minted semantic ids.
 *
 * The engine's semantic-id contract admits `[A-Za-z0-9][A-Za-z0-9._:-]{0,127}`,
 * so raw JSON can never ride in an OperationId, an IdempotencyKey, or a
 * branch id. The browser has no request-identity secret, so instead of the
 * server's HMAC handle the web hashes its id inputs into hex — deterministic,
 * so replaying the same input converges on the id already in play rather than
 * minting a second one for the same thing.
 */
export function webFingerprint(input: string): string {
  return (
    fnv1a64(input, fnvOffsetBasis) +
    fnv1a64(input, fnvOffsetBasis ^ fnvLaneSalt)
  )
}

const fnvOffsetBasis = 0xcbf29ce484222325n
const fnvLaneSalt = 0x9e3779b97f4a7c15n
const fnvPrime = 0x100000001b3n
const fnvMask = 0xffffffffffffffffn

function fnv1a64(input: string, seed: bigint): string {
  let hash = seed & fnvMask
  for (let index = 0; index < input.length; index++) {
    hash ^= BigInt(input.charCodeAt(index))
    hash = (hash * fnvPrime) & fnvMask
  }
  return hash.toString(16).padStart(16, "0")
}
