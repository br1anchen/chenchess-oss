export type JsonPrimitive = string | number | boolean | null

export type JsonValue = JsonPrimitive | JsonObject | JsonValue[]

export type JsonObject = { readonly [key: string]: JsonValue }

export function parseIsString(value: unknown): value is string {
  return typeof value === "string"
}

export function parseIsObject(value: unknown): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}

export type HostFunction = (...args: never[]) => void

export function parseIsFunction(value: unknown): value is HostFunction {
  return typeof value === "function"
}
