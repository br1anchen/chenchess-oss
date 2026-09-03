/**
 * The page's one clipboard write. Surfaces hand it to the affordance that
 * copies, so a test hands a recorder in its place instead of mocking the
 * browser.
 */
export function writeClipboardText(text: string): Promise<void> {
  return navigator.clipboard.writeText(text)
}
