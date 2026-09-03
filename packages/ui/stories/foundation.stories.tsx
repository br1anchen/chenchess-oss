import type { Meta, StoryObj } from "@storybook/react-vite"
import * as stylex from "@stylexjs/stylex"
import { useEffect, useState } from "react"

import { Button } from "@astryxdesign/core/Button"
import { Card } from "@astryxdesign/core/Card"
import { Heading } from "@astryxdesign/core/Heading"
import { Text } from "@astryxdesign/core/Text"
import { TextInput } from "@astryxdesign/core/TextInput"
import { spacingVars } from "@astryxdesign/core/theme/tokens.stylex"
import { VStack } from "@astryxdesign/core/VStack"

import { Dialog, DialogHeader } from "../src/components/dialog"
import { foundationCheckFailure } from "../src/lib/foundationCheckFailure"

const styles = stylex.create({
  page: {
    paddingBlock: spacingVars["--spacing-6"],
    paddingInline: spacingVars["--spacing-8"],
    maxWidth: "48rem",
  },
})

/**
 * The Astryx foundation smoke check — the cascade-order canary.
 *
 * A broken cascade layer order fails silently and identically on every screen:
 * an unlayered rule, or an app layer declared after `astryx-base`, strips
 * padding, borders and focus rings off every component, and keeps a closed
 * dialog laid out over the page where it swallows clicks. This story is itself
 * an Astryx surface — layout primitives, typed tokens, `stylex.create` — so a
 * missing StyleX compiler shows up here as a story with no padding, the same
 * way a broken layer order shows up as a button with none.
 */
const meta = {
  title: "Foundation",
} satisfies Meta
export default meta

type Story = StoryObj

export const AstryxCascadeCheck: Story = {
  render: function AstryxCascadeCheckStory() {
    const [email, setEmail] = useState("")
    const [failure, setFailure] = useState<string>()
    const [checked, setChecked] = useState(false)

    useEffect(() => {
      setFailure(foundationCheckFailure(document))
      setChecked(true)
    }, [])

    return (
      <VStack data-foundation-check gap={4} hAlign="start" xstyle={styles.page}>
        <VStack gap={2} hAlign="start">
          <Text color="secondary" type="label">
            ChenChess
          </Text>
          <Heading level={1} textWrap="balance">
            Astryx foundation check
          </Heading>
          <Text as="p" display="block" type="supporting">
            Primitives below are unstyled by ChenChess. A filled primary button
            with visible padding, a bordered input and a padded card mean the
            cascade layer order holds. Page padding here is authored StyleX — if
            it is missing, the compiler is not in the Vite graph.
          </Text>
        </VStack>

        <Text
          as="p"
          data-foundation-check-result={
            checked ? (failure === undefined ? "ok" : "broken") : undefined
          }
          display="block"
          type="body"
        >
          {failure ??
            "Foundation intact: every primitive kept its own padding."}
        </Text>

        <Button label="Primary action" variant="primary" />
        <TextInput
          label="Email"
          onChange={setEmail}
          placeholder="you@example.com"
          value={email}
        />
        <Card>One card with default padding</Card>

        <Dialog isOpen={false} onOpenChange={() => {}}>
          <DialogHeader title="Closed dialog" />
        </Dialog>
      </VStack>
    )
  },
}
