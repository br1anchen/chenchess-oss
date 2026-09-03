/**
 * The migration guide's foundation assertion, in the two shapes a broken
 * cascade layer order actually takes.
 *
 * A primitive that lost its own padding means an app stylesheet is outranking
 * `astryx-base`, either by staying unlayered or by landing in a layer declared
 * after it. A closed dialog that is still laid out means the same thing about
 * one specific declaration — `display` — and it costs more than looks: the
 * dialog stays at `opacity: 0` and `position: fixed`, so it is invisible while
 * it swallows every click inside its box.
 *
 * Returns the reason the foundation is broken, or `undefined` when it holds.
 * Callers that need a hard failure throw on the returned string. FoundationCheck
 * writes the result onto `data-foundation-check-result`; verifyFoundation waits
 * for that attribute instead of serializing this function into the page.
 */
export function foundationCheckFailure(scope: Document | Element) {
  const root = scope.querySelector("[data-foundation-check]")
  if (!root) return "the foundation check page did not render"
  const pagePadding = getComputedStyle(root).paddingInline
  if (pagePadding === "" || pagePadding.startsWith("0px")) {
    return (
      "foundation broken: the page has no StyleX padding, so the StyleX " +
      "compiler is missing from the Vite graph or its output is being " +
      "overridden. Every ChenChess Vite config must run chenStylexVitePlugin " +
      "before the React plugin."
    )
  }
  const button = root.querySelector("button")
  if (!button) return "the foundation check page rendered no button"
  const { paddingInline } = getComputedStyle(button)
  if (paddingInline === "" || paddingInline.startsWith("0px")) {
    return (
      "foundation broken: an unlayered reset or a later cascade layer is " +
      "overriding component styles, so the button has no inline padding. " +
      "Check that every app stylesheet is assigned a layer below astryx-base."
    )
  }
  const closedDialog = root.querySelector("dialog:not([open])")
  if (!closedDialog)
    return "the foundation check page rendered no closed dialog"
  if (getComputedStyle(closedDialog).display !== "none") {
    return (
      "foundation broken: a closed dialog is still laid out, so an app rule " +
      "is overriding the `display: none` Astryx gives it. Move the rule into " +
      "a layer below astryx-base, or style the dialog's own content wrapper " +
      "instead of the dialog element."
    )
  }
  return undefined
}
