import { HStack, Link } from "@chenchess/ui"

export function PublicLegalLinks() {
  return (
    <HStack aria-label="Legal and support" as="nav" gap={3}>
      <Link href="/privacy/">Privacy</Link>
      <Link href="/terms/">Terms</Link>
      <Link href="/support/">Support</Link>
    </HStack>
  )
}
