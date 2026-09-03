/**
 * jsdom does not implement `CSS.escape` or the native dialog modal methods
 * Astryx Dialog uses. One shim, imported by every Vitest graph that renders
 * those components.
 */
export function installAstryxTestShims() {
  if (globalThis.ResizeObserver == null) {
    globalThis.ResizeObserver = class ResizeObserver {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
  }

  const view = globalThis.window
  if (view != null && view.matchMedia == null) {
    Object.defineProperty(view, "matchMedia", {
      configurable: true,
      writable: true,
      value: (query: string) => ({
        matches: false,
        media: query,
        onchange: null,
        addEventListener() {},
        addListener() {},
        dispatchEvent() {
          return false
        },
        removeEventListener() {},
        removeListener() {},
      }),
    })
  }

  Object.defineProperty(globalThis, "CSS", {
    configurable: true,
    value: {
      escape(value: string) {
        return String(value).replace(/[^a-zA-Z0-9_-]/g, (char) => `\\${char}`)
      },
    },
  })

  const dialogElement = globalThis.HTMLDialogElement
  if (!dialogElement) return
  dialogElement.prototype.showModal = function showModal() {
    this.setAttribute("open", "")
  }
  dialogElement.prototype.close = function close() {
    this.removeAttribute("open")
  }
}
