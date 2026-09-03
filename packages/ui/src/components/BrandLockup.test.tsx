// @vitest-environment jsdom

import { render, screen } from "@testing-library/react"
import { describe, expect, test } from "vitest"

import { BrandLockup, PRODUCT_NAME } from "./BrandLockup"

describe("the product wordmark", () => {
  test("names the product and links home when given an address", () => {
    render(<BrandLockup href="/" />)

    const link = screen.getByRole("link", { name: PRODUCT_NAME })
    expect(link.getAttribute("href")).toBe("/")
  })

  test("renders as plain text when it is not a link", () => {
    const { container } = render(<BrandLockup label="Example Chess" />)

    expect(container.querySelector("a")).toBeNull()
    expect(screen.getByText("Example Chess")).toBeTruthy()
  })
})
