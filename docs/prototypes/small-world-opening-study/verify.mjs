/**
 * Walk the prototype end to end in a real browser and fail on any page error.
 *
 * Run from the repository root: bun docs/prototypes/small-world-opening-study/verify.mjs
 */
import { chromium } from "playwright"
import fs from "node:fs"

const dir = "docs/prototypes/small-world-opening-study"
const worlds = JSON.parse(fs.readFileSync(`${dir}/worlds.json`, "utf8"))
const browser = await chromium.launch({ channel: "chrome" })
const page = await browser.newPage({ viewport: { width: 1180, height: 940 } })
const errors = []
page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`))
page.on("console", (m) => {
  if (m.type() === "error") errors.push(`console: ${m.text()}`)
})
await page.goto(`file://${process.cwd()}/${dir}/small-world-opening-study.html`)

for (const world of worlds) {
  await page.click(`text=${world.name}`)
  for (const slot of world.slots) {
    await page.click(`[data-slot="${slot.square}"]`)
    await page
      .locator(".sq")
      .nth(squareIndex(slot.accepts[0], world.side === "black"))
      .click()
  }
  await page.click("#advance")
  await page.fill("#plantext", "A plan, in the learner's own words.")
  await page.click("#gradeplan")
  await page.click("#advance")
  await page.click(
    `[data-break="${world.breaks.find((b) => b.verdict === "primary").san}"]`,
  )
  await page.click("#advance")
  for (const [index, deviation] of world.deviations.entries()) {
    if (index > 0) await page.click("#advance")
    await page.click(`[data-dev="${deviation.answer}"]`)
  }
  await page.click("#advance")

  const expected = world.slots.length + 1 + world.deviations.length
  const tally = await page.textContent(".tally")
  const got = tally.match(/(\d+)\/(\d+)/)
  if (got[1] !== String(expected) || got[2] !== String(expected)) {
    errors.push(`${world.id}: expected ${expected}/${expected}, got ${got[0]}`)
  }
  console.log(`${world.id}: ${got[0]}`)
}

console.log(
  errors.length ? `FAIL\n${errors.join("\n")}` : "OK — no page errors",
)
await browser.close()
process.exit(errors.length ? 1 : 0)

function squareIndex(name, flip) {
  const files = flip ? [..."abcdefgh"].reverse() : [..."abcdefgh"]
  const ranks = flip ? [1, 2, 3, 4, 5, 6, 7, 8] : [8, 7, 6, 5, 4, 3, 2, 1]
  return ranks.indexOf(Number(name[1])) * 8 + files.indexOf(name[0])
}
