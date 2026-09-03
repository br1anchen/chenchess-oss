import { createHash } from "node:crypto";

const canonicalSanitizedPgnSha256 =
  "8e9fa7ba48f3003108202c1c0b9f50b1fe6cce3d004fd7617dd87e630dc23744";

export function canonicalSanitizedConformancePgn(pgn: string): string {
  const sanitized = sanitizeConformancePgn(pgn);
  const digest = createHash("sha256").update(sanitized).digest("hex");
  if (digest !== canonicalSanitizedPgnSha256) {
    throw new Error("conformance fixture does not match canonical Synthet1");
  }
  return sanitized;
}

function sanitizeConformancePgn(pgn: string): string {
  return pgn
    .replaceAll("\r\n", "\n")
    .replace(/\n+$/, "\n\n")
    .split("\n")
    .filter(
      (line) =>
        !line.startsWith("[BlackRatingDiff ") &&
        !line.startsWith("[WhiteRatingDiff "),
    )
    .map((line) => {
      const match = /^\[([A-Za-z0-9_]+)\s+"(?:[^"\\]|\\.)*"\]$/.exec(line);
      const replacement = match ? replacementHeader(match[1]!) : undefined;
      return replacement === undefined
        ? line
        : `[${match![1]} "${replacement}"]`;
    })
    .join("\n");
}

function replacementHeader(name: string): string | undefined {
  switch (name) {
    case "Black":
      return "Benchmark Black";
    case "GameId":
      return "benchmark-fixture";
    case "Site":
      return "https://benchmark.invalid/game";
    case "UTCDate":
      return "2026.01.01";
    case "UTCTime":
      return "00:00:00";
    case "White":
      return "Benchmark White";
    default:
      return undefined;
  }
}
