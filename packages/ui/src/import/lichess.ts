export type LichessInput =
  | {
      kind: "invalid"
      message: string
    }
  | {
      kind: "bare"
      url: string
    }
  | {
      kind: "qualified"
      url: string
      side: "white" | "black"
    }

const lichessUrl =
  /^https:\/\/lichess\.org\/([A-Za-z0-9]{8})(?:[A-Za-z0-9]{4})?(?:\/(white|black))?$/

export function parseLichessInput(input: string): LichessInput {
  const url = input.trim()
  const match = lichessUrl.exec(url)
  if (!match) {
    return {
      kind: "invalid",
      message:
        "Use one completed game URL such as https://lichess.org/Synthet1 or add /white or /black.",
    }
  }
  const side = match[2]
  return side === "white" || side === "black"
    ? { kind: "qualified", url, side }
    : { kind: "bare", url }
}
