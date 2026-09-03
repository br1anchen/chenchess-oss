import { Link } from "@astryxdesign/core/Link"
import { Text } from "@astryxdesign/core/Text"

export const PRODUCT_NAME = "ChenChess"

export type BrandLockupSize = "header" | "footer" | "workspace"
export type BrandLockupMark = "icon" | "seal" | "none"

export type BrandLockupProps = {
  className?: string
  href?: string
  label?: string
  /** Accepted and ignored: this snapshot ships a wordmark and no mark art. */
  mark?: BrandLockupMark
  size?: BrandLockupSize
}

/**
 * The product wordmark.
 *
 * This snapshot carries no mark artwork. The name is the product's, and
 * `TRADEMARKS.md` says what may be done with it; a fork should put its own name
 * here rather than inherit one.
 */
export function BrandLockup({
  className,
  href,
  label = PRODUCT_NAME,
  size = "header",
}: BrandLockupProps) {
  const wordmark = (
    <Text type={size === "footer" ? "supporting" : "body"} weight="bold">
      {label}
    </Text>
  )
  if (!href) return <span className={className}>{wordmark}</span>
  return (
    <Link className={className} href={href}>
      {wordmark}
    </Link>
  )
}
