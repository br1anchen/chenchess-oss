import { BrandLockup, HStack, VStack } from "@chenchess/ui"
import * as stylex from "@stylexjs/stylex"
import type { ReactNode } from "react"

import { chromeStyles } from "./publicChrome.styles"
import { publicUtilityStyles } from "./publicUtility.styles"

export type PublicNavPage = "privacy" | "terms" | "support"

const navItems: ReadonlyArray<{
  href: string
  label: string
  page: PublicNavPage
}> = [
  { href: "/privacy/", label: "Privacy", page: "privacy" },
  { href: "/terms/", label: "Terms", page: "terms" },
  { href: "/support/", label: "Support", page: "support" },
]

export function PublicUtilityPage({
  children,
  current,
}: {
  children: ReactNode
  current?: PublicNavPage
}) {
  return (
    <VStack gap={0} hAlign="stretch">
      <a href="#main-content" {...stylex.props(chromeStyles.skipLink)}>
        Skip to content
      </a>
      <header {...stylex.props(chromeStyles.header)}>
        <BrandLockup href="/" size="header" />
        <PublicNav
          current={current}
          label="Public pages"
          linkStyle={chromeStyles.navLink}
          xstyle={chromeStyles.nav}
        />
      </header>
      <VStack
        as="main"
        gap={6}
        hAlign="stretch"
        id="main-content"
        xstyle={publicUtilityStyles.main}
      >
        {children}
      </VStack>
      <footer {...stylex.props(chromeStyles.footer)}>
        <HStack vAlign="center" xstyle={chromeStyles.footerBrand}>
          <BrandLockup mark="none" size="footer" />
        </HStack>
        <PublicNav
          current={current}
          label="Legal and support"
          linkStyle={chromeStyles.footerLink}
          xstyle={chromeStyles.footerNav}
        />
      </footer>
    </VStack>
  )
}

function PublicNav({
  current,
  label,
  linkStyle,
  xstyle,
}: {
  current?: PublicNavPage
  label: string
  linkStyle: stylex.StyleXStyles
  xstyle: stylex.StyleXStyles
}) {
  return (
    <nav aria-label={label} {...stylex.props(xstyle)}>
      {navItems.map((item) => (
        <a
          aria-current={item.page === current ? "page" : undefined}
          href={item.href}
          key={item.page}
          {...stylex.props(linkStyle)}
        >
          {item.label}
        </a>
      ))}
    </nav>
  )
}
