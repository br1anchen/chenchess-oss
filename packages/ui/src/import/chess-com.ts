import { CHESS_COM_GAME_URL_PATTERN } from "@chenchess/coach-engine-sdk/chess-com-url"

export type ChessComInput =
  | {
      kind: "invalid"
      message: string
    }
  | {
      kind: "ready"
      url: string
    }

const chessComGameUrl = new RegExp(CHESS_COM_GAME_URL_PATTERN)

export function parseChessComInput(input: string): ChessComInput {
  const url = input.trim()
  return chessComGameUrl.test(url)
    ? { kind: "ready", url }
    : {
        kind: "invalid",
        message:
          "Use one shared Chess.com game URL such as https://www.chess.com/game/daily/100000000002.",
      }
}
