import { useEffect, useRef } from "react"

import { Text, WatercolorCard } from "@chenchess/ui"

import { AuthStudio } from "./AuthStudio"

export type Navigate = (href: string) => void

export function RouteRedirect({
  description,
  href,
  navigate,
  title,
}: {
  description: string
  href: string
  navigate: Navigate
  title: string
}) {
  const lastNavigatedHref = useRef<string | null>(null)

  useEffect(() => {
    if (lastNavigatedHref.current === href) return
    lastNavigatedHref.current = href
    navigate(href)
  }, [href, navigate])

  return (
    <AuthStudio legal={false}>
      <WatercolorCard headingLevel={2} title={title}>
        <Text as="p" display="block" type="body">
          {description}
        </Text>
      </WatercolorCard>
    </AuthStudio>
  )
}
