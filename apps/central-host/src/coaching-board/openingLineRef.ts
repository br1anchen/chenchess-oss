/**
 * An Opening Line address: `<eco>-<name-slug>-<digest4>` over the catalog
 * row's move path. The digest is identity; the slug is legibility.
 *
 * v1 mints digest4 as the first four hex characters of an FNV-1a 32-bit hash
 * of the normalized PGN path. #493 aligns the engine root to the same
 * constructor or replaces it in one place.
 */
export type OpeningLineRef = string & {
  readonly __openingLineRef: unique symbol
}

const openingLineRefPattern =
  /^([A-E][0-9]{2})-([a-z0-9]+(?:-[a-z0-9]+)*)-([a-f0-9]{4})$/

export type ParsedOpeningLineRef = {
  digest4: string
  eco: string
  nameSlug: string
  ref: OpeningLineRef
}

export function parseOpeningLineRef(value: string): OpeningLineRef | undefined {
  return openingLineRefPattern.test(value) ? asOpeningLineRef(value) : undefined
}

export function readOpeningLineRef(
  value: string,
): ParsedOpeningLineRef | undefined {
  const match = openingLineRefPattern.exec(value)
  if (!match || !match[1] || !match[2] || !match[3]) return undefined
  return {
    digest4: match[3],
    eco: match[1],
    nameSlug: match[2],
    ref: asOpeningLineRef(value),
  }
}

export function openingLineRefFromPath(
  eco: string,
  name: string,
  path: string,
): OpeningLineRef {
  return asOpeningLineRef(
    `${eco.toUpperCase()}-${openingNameSlug(name)}-${openingLineDigest4(path)}`,
  )
}

export function openingNameSlug(name: string) {
  const slug = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
  return slug.length > 0 ? slug : "opening"
}

export function openingLineDigest4(path: string) {
  let hash = 2166136261
  for (const character of path) {
    hash ^= character.charCodeAt(0)
    hash = Math.imul(hash, 16777619)
  }
  return (hash >>> 0).toString(16).padStart(8, "0").slice(0, 4)
}

export function openingLineTitle(ref: OpeningLineRef) {
  const parsed = readOpeningLineRef(ref)
  if (!parsed) return "Opening Line"
  return `${parsed.eco} · ${titleFromSlug(parsed.nameSlug)}`
}

function titleFromSlug(slug: string) {
  return slug
    .split("-")
    .map((part) =>
      part[0] ? `${part[0].toUpperCase()}${part.slice(1)}` : part,
    )
    .join(" ")
}

function asOpeningLineRef(value: string): OpeningLineRef {
  // SAFETY: the caller matched openingLineRefPattern or built the three
  // segments from eco / slug / digest4.
  return value as OpeningLineRef
}
