# GitHub issue constraints for Review Feedback Reports

Research date: 2026-07-13

This note uses GitHub's documentation and first-party API schemas only.

## Findings

### GitHub does not document a numeric issue-body limit

The REST `Create an issue` endpoint defines `body` only as a string containing the issue contents. Its published schema gives no `maxLength`. The GraphQL `CreateIssueInput` likewise exposes `body` as a `String` without a numeric limit. Neither source supports treating commonly quoted values such as 65,535 or 65,536 characters as a stable GitHub contract. [REST API: Create an issue](https://docs.github.com/en/rest/issues/issues#create-an-issue), [GraphQL: CreateIssueInput](https://docs.github.com/en/graphql/reference/issues#createissueinput)

This does not mean bodies are unlimited. It means the limit is unspecified. Chenchess should enforce and test its own payload budget rather than depend on an undocumented ceiling.

### Prefilled new-issue URLs have an unspecified, smaller constraint

GitHub documents these standard query parameters for `/issues/new`: `title`, `body`, `labels`, `milestone`, `assignees`, `projects`, and `template`. Metadata parameters work only when the reporter has the matching permission. Invalid or unauthorized parameter combinations may return `404`. An overlong URL returns `414 URI Too Long`, but GitHub publishes no byte or character threshold. [Creating an issue from a URL query](https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/creating-an-issue#creating-an-issue-from-a-url-query)

Issue Forms also accept query parameters for custom text fields. The form element's `id` is the query-parameter name. The same unspecified URL-length limit still applies. [GitHub form schema: element keys](https://docs.github.com/en/communities/using-templates-to-encourage-useful-issues-and-pull-requests/syntax-for-githubs-form-schema#keys)

Practical conclusion: a prefilled URL is suitable for a short title, label, template name, and perhaps a small field. It is not a reliable transport for the full Review Feedback Report. URL encoding also expands many JSON and PGN characters, so the encoded URL can be materially larger than the report. This conclusion is an inference from GitHub's documented `414` behavior; GitHub does not publish a safe cross-browser threshold.

### A GitHub issue can contain a machine-readable fenced JSON report

GitHub issue bodies use Markdown and support fenced code blocks with an optional language identifier. A classic Markdown issue template can therefore contain or instruct the reporter to paste one fenced `json` block. [Creating and highlighting code blocks](https://docs.github.com/en/get-started/writing-on-github/working-with-advanced-formatting/creating-and-highlighting-code-blocks), [About issue templates](https://docs.github.com/en/communities/using-templates-to-encourage-useful-issues-and-pull-requests/about-issue-and-pull-request-templates)

An Issue Form can make the contract more explicit:

```yaml
- type: textarea
  id: report
  attributes:
    label: Review Feedback Report
    render: json
  validations:
    required: true
```

For a `textarea`, `render` wraps submitted text in a code block, and `json` is a supported language identifier. GitHub converts all submitted Issue Form inputs to an ordinary Markdown issue body. The `report` field can also be prefilled with a `report=<URL-encoded value>` query parameter, subject to the URL caveat above. [GitHub form schema: textarea](https://docs.github.com/en/communities/using-templates-to-encourage-useful-issues-and-pull-requests/syntax-for-githubs-form-schema#textarea), [About issue templates and Issue Forms](https://docs.github.com/en/communities/using-templates-to-encourage-useful-issues-and-pull-requests/about-issue-and-pull-request-templates)

GitHub formats the block but does not validate that its contents are valid JSON or match the Chenchess schema. The reproduction CLI must do both.

Two Issue Form limitations matter here:

- Issue Forms remain in public preview and may change.
- `validations.required` prevents submission only for public repositories. While the development repository is private, the repository cannot depend on the browser form to require the report field. [Configuring issue templates](https://docs.github.com/en/communities/using-templates-to-encourage-useful-issues-and-pull-requests/configuring-issue-templates-for-your-repository), [GitHub form schema: textarea validations](https://docs.github.com/en/communities/using-templates-to-encourage-useful-issues-and-pull-requests/syntax-for-githubs-form-schema#validations-for-textarea)

### Raw Markdown is available for deterministic parsing

The REST Issues API returns raw Markdown in `body` by default and explicitly supports the `application/vnd.github.raw+json` media type. An agent can fetch the issue, locate the single fenced `json` block under a stable heading, and parse it without scraping rendered HTML. [REST API custom media types for issues](https://docs.github.com/en/rest/issues/issues)

## Recommended v1 contract

1. Generate the complete human summary plus fenced Review Feedback Report JSON in the application and copy it to the clipboard.
2. Open GitHub with a short prefilled URL containing only `title`, `labels`, and `template` or the Issue Form name.
3. Have the reporter paste the generated text into the issue or the form's `report` textarea and submit manually.
4. Fetch raw Markdown during triage. Require exactly one `json` fence, validate JSON syntax and `schemaVersion`, then validate the full report schema.
5. Reject oversized reports using a Chenchess-owned size limit chosen from real generated fixtures. GitHub's public documentation does not supply a numeric limit to inherit.

## Remaining uncertainty

GitHub does not document:

- the maximum issue-body size;
- the maximum accepted `/issues/new` URL length;
- stable byte-for-byte Markdown emitted by Issue Forms; or
- how `body` and `template` interact when both appear in one new-issue URL.

The parser should therefore depend only on the fenced JSON marker and schema, not on surrounding Issue Form headings or whitespace. The integration test should cover the actual GitHub flow before release.
