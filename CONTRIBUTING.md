# Contributing

## Read this first: it is a snapshot

This repository is a **point-in-time snapshot** of ChenChess, published so the
work can be read and run. It has no history — one squashed commit — and it is
not where the product is developed. Development continues in a private
repository, and the intent is eventually to open that one instead.

So: pull requests here cannot be merged upstream in the ordinary way, and
issues are turned off. That is not a judgement about your contribution; it is
that this tree has no upstream to merge into.

If you want to change something, the useful moves are, in order:

1. **Fork it.** The AGPL says you may, and the snapshot is complete enough to
   run — `bun run local:up` brings up the whole product on your machine with no
   deployment and no Google credential.
2. **Tell the maintainer what you found**, through
   [github.com/br1anchen](https://github.com/br1anchen). A described bug in a
   named file is worth more here than a patch nobody can merge.

## If a contribution is accepted

Anything taken from this repository into ChenChess is taken under a
**Contributor Licence Agreement**, not a Developer Certificate of Origin. This
matters and is worth stating plainly rather than burying:

A DCO says you had the right to send what you sent. It grants no power to
relicense it. The first contribution merged under a DCO into an AGPL codebase
fixes that codebase on the AGPL permanently, because every later relicensing
would need that contributor's separate consent. The private repository is not
AGPL today, and whether it becomes so is an open decision — so a DCO here would
quietly decide it.

The CLA therefore asks for the thing a DCO does not give: a licence broad
enough that the project can change its own licence later. You keep your
copyright. You are not assigning anything.

The agreement is not drafted yet. It will be linked here before any
contribution is accepted, and until then nothing is merged, so nothing is taken
under terms you have not read.

## Running it

`README.md` has the whole path: prerequisites, `bun run local:up`,
`bun run local:seed`, and what each of the five processes is for.

## Reporting something sensitive

If you find a security problem, do not open it in public. Contact the
maintainer through [github.com/br1anchen](https://github.com/br1anchen) and
describe the class of problem rather than posting a working exploit.
