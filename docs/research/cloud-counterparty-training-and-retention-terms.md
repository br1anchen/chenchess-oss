# Training and retention terms of the admissible ZDR cloud counterparties

Research date: 2026-08-12 (every quotation below was fetched on this date from the URL recorded
beside it; vendor terms carry a posted version date and are amendable on notice, so **each quote is a
snapshot that must be re-fetched and re-dated at pin time**, not a durable commitment).

Research asset for [Verify the training and retention terms of the admissible ZDR counterparties](#330),
under [Ship the tailored OpenRouter web Language Layer to beta](#229).
Companion to [OpenRouter constraints for the web Language Layer](./openrouter-web-language-layer-constraints.md),
whose §3.4 recorded Anthropic and OpenAI first-party terms as verified and **Google Vertex as
UNVERIFIED**. This document closes that gap and opens two new ones.

## Question

The pin posture fixed by issue #294 routes every Language Layer request to exactly one model on
exactly one provider endpoint, with `allow_fallbacks: false`, `data_collection: "deny"`, `zdr: true`,
and the endpoint verified present in `GET /api/v1/endpoints/zdr` at pin time. Pinning a **cloud**
endpoint substitutes the **cloud vendor's** service terms for the **model vendor's** no-training
commitment, so the operative counterparty's own terms must be read and recorded as a precondition of
setting the pin.

The admissible routes are:

| Model | Endpoint family |
| --- | --- |
| `anthropic/claude-haiku-4.5` | `amazon-bedrock` |
| `anthropic/claude-sonnet-5` | `amazon-bedrock` or `azure` |
| `openai/gpt-5.6-luna` | `azure` |
| `google/gemini-3.5-flash-lite` | `google-vertex` |

So: for **AWS Bedrock**, **Microsoft Azure**, and **Google Cloud Vertex AI** — what does the
operative *contract* (not the documentation) say about training on inputs and outputs, about
retention of prompts and completions, and about abuse-detection logging; can each be verified from
primary sources; and is each therefore pinnable?

## Sources and method

Only first-party vendor sources were used — service terms, product terms, DPAs, and the vendors' own
documentation sites. No blog posts, no third-party summaries, no vendor recaps. Every document was
retrieved directly (HTTP `GET`, redirects followed) and quoted from the retrieved body; search was
used only to locate canonical URLs.

Throughout, three evidence grades are used and kept distinct:

- **Contract** — a term in a document that the customer agreement incorporates (service terms,
  product terms, DPA, an EULA incorporated by reference).
- **Documentation, contract-referenced** — a docs page that a contractual clause explicitly points
  at, so its content is operative for the clause that points at it, but which the vendor can revise
  without a terms-change notice.
- **Documentation only** — a docs or FAQ page with no contractual hook. Informative, not binding.
  A claim that exists only here is a **material weakness for a pin precondition**.

### Documents fetched (all 2026-08-12)

**AWS**

| Document | Resolving URL | Version |
| --- | --- | --- |
| AWS Service Terms (§1.14 Data Protection, §50 AWS Machine Learning and Artificial Intelligence Services, §50.12 Amazon Bedrock) | `https://aws.amazon.com/service-terms/` | "Last Updated: July 29, 2026" |
| AWS Customer Agreement (§1.4 Data Privacy) | `https://aws.amazon.com/agreement/` | "Last Updated: June 01, 2026" |
| Serverless Third-Party Models on Amazon Bedrock — seller EULAs, incl. "Anthropic on Bedrock – Commercial Terms of Service" | `https://aws.amazon.com/legal/bedrock/third-party-models/` | no version date on page |
| AWS Data Processing Addendum (incorporated by ST §1.14.1) | `https://d1.awsstatic.com/legal/aws-dpa/aws-dpa.pdf` | link resolved; PDF body not parsed |
| Amazon Bedrock User Guide — Abuse detection | `https://docs.aws.amazon.com/bedrock/latest/userguide/abuse-detection.html` | undated |
| Amazon Bedrock User Guide — Data protection | `https://docs.aws.amazon.com/bedrock/latest/userguide/data-protection.html` | undated |
| Amazon Bedrock User Guide — Data retention | `https://docs.aws.amazon.com/bedrock/latest/userguide/data-retention.html` | undated |
| Amazon Bedrock FAQs (Security) | `https://aws.amazon.com/bedrock/faqs/` | undated |

**Microsoft**

| Document | Resolving URL | Version |
| --- | --- | --- |
| Microsoft Product Terms — Universal License Terms for Online Services (incl. "Microsoft Generative AI Services") | `https://www.microsoft.com/licensing/terms/product/ForOnlineServices/all` | effective-date selector; newest option **8/10/2026** |
| Microsoft Product Terms — Microsoft Azure product offering terms (incl. "Microsoft Foundry Models", "Limited Access Services") | `https://www.microsoft.com/licensing/terms/productoffering/MicrosoftAzure/EAEAS` | same selector, newest **8/10/2026** |
| Microsoft Products and Services Data Protection Addendum | `https://www.microsoft.com/licensing/docs/view/Microsoft-Products-and-Services-Data-Protection-Addendum-DPA` → `…/download/MicrosoftProductandServicesDPA(WW)(English)(May2026)(CR).docx` | **May 2026 (WW, English)** |
| Data, privacy, and security for Models sold by Azure in Microsoft Foundry | `https://learn.microsoft.com/en-us/azure/foundry/responsible-ai/openai/data-privacy` | "Last updated on 2026-05-19" |
| Abuse monitoring (Foundry Models sold by Azure) | `https://learn.microsoft.com/en-us/azure/foundry/openai/concepts/abuse-monitoring` | "Last updated on 2026-05-19" |
| Limited access for Foundry Models sold by Azure | `https://learn.microsoft.com/en-us/azure/foundry/responsible-ai/openai/limited-access` | "Last updated on 2026-05-19" |
| Foundry Models sold by Azure (model list) | `https://learn.microsoft.com/en-us/azure/foundry/foundry-models/concepts/models-sold-directly-by-azure` | undated |
| Foundry Models from partners and community | `https://learn.microsoft.com/en-us/azure/foundry/foundry-models/concepts/models-from-partners` | undated |
| Data, privacy, and security for Claude models in Microsoft Foundry | `https://learn.microsoft.com/en-us/azure/foundry/responsible-ai/claude-models/data-privacy` | "Last updated on 2026-06-29" |
| Compare hosting options for Claude models in Microsoft Foundry | `https://learn.microsoft.com/en-us/azure/foundry/foundry-models/concepts/claude-models-hosting-comparison` | undated |

**Google**

| Document | Resolving URL | Version |
| --- | --- | --- |
| Google Cloud Platform Terms of Service (§4.3 Generative AI Safety and Abuse for GCP Services) | `https://cloud.google.com/terms/` | no current-version date printed; archive list published |
| Google Cloud Service Specific Terms (§18 Training Restriction, §19 Separate Offerings, §20 Generative AI Services) | `https://cloud.google.com/terms/service-terms` | no current-version date printed; newest archived version **April 22, 2026** |
| Cloud Data Processing Addendum | `https://cloud.google.com/terms/data-processing-addendum` | fetched; relied on only as a pointer |
| Gemini Enterprise Agent Platform and zero data retention | `https://docs.cloud.google.com/gemini-enterprise-agent-platform/resources/zero-data-retention` | undated |
| Abuse monitoring | `https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/abuse-monitoring` | undated |

**Anthropic** (operative for two of the routes, see below)

| Document | Resolving URL |
| --- | --- |
| Anthropic Commercial Terms of Service | `https://www.anthropic.com/legal/commercial-terms` |
| Anthropic Data Processing Addendum | `https://www.anthropic.com/legal/data-processing-addendum` |

### Redirects, 404s, and retrieval problems encountered

These are findings, not housekeeping — two of them explain the prior UNVERIFIED verdict and one of
them changes which document is operative.

1. **The Vertex data-governance URL has moved twice.**
   `https://cloud.google.com/vertex-ai/generative-ai/docs/data-governance` now resolves, after
   redirects, to
   `https://docs.cloud.google.com/gemini-enterprise-agent-platform/resources/zero-data-retention`.
   The intermediate host recorded in the companion doc,
   `https://docs.cloud.google.com/vertex-ai/generative-ai/docs/data-governance`, redirects to the
   **same** final page. The content was not deleted — the Vertex generative-AI docs set has been
   re-homed under **"Gemini Enterprise Agent Platform"**, and the governance page has been retitled
   around zero data retention. The operative statements *are* on the final page; they were simply not
   at the URL previously fetched. **The prior UNVERIFIED status for Google is resolved.**
2. `https://cloud.google.com/vertex-ai/generative-ai/docs/learn/abuse-monitoring` likewise resolves
   to `https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/abuse-monitoring`.
3. **Microsoft has re-homed the Foundry docs from `/azure/ai-foundry/…` to `/azure/foundry/…`.**
   `https://learn.microsoft.com/en-us/legal/cognitive-services/openai/data-privacy` and
   `https://learn.microsoft.com/en-us/azure/ai-foundry/responsible-ai/openai/data-privacy` both
   redirect to `https://learn.microsoft.com/en-us/azure/foundry/responsible-ai/openai/data-privacy`.
   Guessed paths under the old prefix 404 outright (`/azure/ai-foundry/concepts/models-sold-directly-by-azure`,
   `/azure/ai-foundry/how-to/deploy-models-anthropic`, `/azure/ai-foundry/responsible-ai/anthropic/data-privacy`
   all returned **HTTP 404**). Any pin record must store the `/azure/foundry/…` form.
4. **The AWS Service Terms page cannot be read by summarising fetch.** A prompt-driven fetch of
   `https://aws.amazon.com/service-terms/` reported that "there is no dedicated Section 50 for Amazon
   Bedrock" — false. The page is ~1 MB of HTML and the summariser truncated before reaching §50.
   The document was retrieved raw and §50 extracted directly. **Do not trust a summarised read of
   this page.**
5. **The Microsoft DPA is a `.docx`, not a web page.** The DPA landing page is a version index; the
   operative text is the downloadable "Microsoft Product and Services DPA (WW) (English) (May 2026)"
   document, which was downloaded and parsed.
6. **Neither Google terms page prints a current-version date.** Both publish an archive list of prior
   versions; the newest archived Service Specific Terms entry is April 22, 2026, which bounds but does
   not identify the live version. The pin record therefore has to cite Google terms by **fetch date +
   quoted text**, not by version.

---

## Answer

### Verdict table

| Counterparty | Trains on inputs/outputs? | Retention of prompts/completions | Abuse logging: scope / retention / opt-out | Operative document | Verifiable from primary sources? | Pinnable? |
| --- | --- | --- | --- | --- | --- | --- |
| **AWS Bedrock** (`claude-haiku-4.5`, `claude-sonnet-5`) | No. AWS has no contractual grant to use Bedrock content for improvement (§50.3 list excludes Bedrock) and is barred from other use by AWS Customer Agreement §1.4. Anthropic separately commits "Anthropic may not train models on Customer Content from Services" via the EULA incorporated by ST §50.12.1. Inputs **and** outputs covered (ST §50.2: output is Your Content). | Zero by default. ST §50.12.2 authorises up to 30 days **only** "for certain models identified on the Bedrock abuse detection page"; that page lists OpenAI GPT-5.4/5.5/5.6 and Claude Fable 5 — **neither Claude Haiku 4.5 nor Claude Sonnet 5 appears**. | Scope: model-list-gated, per the contract-referenced abuse page. Retention: up to 30 days for listed models; **none** for our two models. Access: AWS only; sharing with the model provider is **opt-in** (ST §50.12.2.2). Opt-out: `data_retention_mode: "none"` is a self-service account/project API setting — no approval — *except* for models that require retention, where ZDR is approval-gated via the AWS account team. | **AWS Service Terms §50.12** + **AWS Customer Agreement §1.4** + the Anthropic-on-Bedrock EULA at `aws.amazon.com/legal/bedrock/third-party-models/` (all contract) | **Yes** | **Yes** |
| **Microsoft Azure** — `openai/gpt-5.6-luna` | No, contractually: "Microsoft Generative AI Services will not use Customer Data to train any generative AI foundation model, except pursuant to Customer's documented instructions", and "Output Content is Customer Data". Binds **Microsoft**; is **silent** on OpenAI. The "not available to OpenAI" claim is documentation only. | **Not stated anywhere.** The contract says Microsoft "will temporarily store Input and Output Content" for abuse monitoring, with **no duration**; the documentation gives no number either. | Scope: all traffic ("temporarily store Input and Output Content"). Retention: **undisclosed**. Access: authorised Microsoft employees, on flagged data, via SAW + JIT approval. Opt-out: "modified abuse monitoring" — a **Limited Access** registration requiring Microsoft approval *and* being "managed by a Microsoft account team or under an eligible program". **Not available by default, and per-subscription.** | **Microsoft Product Terms — Universal License Terms for Online Services, "Microsoft Generative AI Services"** + **Azure product offering terms, "Microsoft Foundry Models → Foundry Models sold by Azure → Data Use and Access for Abuse Monitoring"** (both contract) | **Training: yes. Retention: no.** | **Conditionally** — pin only with the undisclosed retention window recorded as an accepted residual risk |
| **Microsoft Azure** — `anthropic/claude-sonnet-5` | Microsoft's no-training clause **does not apply**: Claude is not a Foundry Model sold by Azure, it is a Non-Microsoft Product. Anthropic's own commitment applies instead ("Anthropic may not train models on Customer Content from Services"). | **Not stated.** Microsoft's own doc routes retention to "Anthropic's Data Processing Addendum and Anthropic's Commercial Terms of Service", and **neither states a retention period**. | Not Microsoft's abuse monitoring at all. Documentation: "Automatic safeguards flag content that might be sent to Anthropic Trust & Safety for review… on an exceptions-only basis." No retention period, no access description, **no documented opt-out**. | Anthropic Commercial ToS + Anthropic DPA, with Microsoft as infrastructure/billing provider under the Microsoft DPA. The Azure Product Terms reach only Marketplace/billing data. | **No** | **No** — see below |
| **Google Vertex AI** (`gemini-3.5-flash-lite`) | No, contractually: "Google will not use Customer Data to train or fine-tune any AI/ML models without Customer's prior permission or instruction." Outputs covered: "Generated Output is Customer Data." Google is both cloud vendor and model vendor here, so there is no third party to cover. | Contractual: "Absent Customer's prior permission or instruction, Google will not store outside Customer's Account (i) Customer Data prompted to a Generative AI Service for longer than is reasonably necessary to create the Generated Output, or (ii) the Generated Output." In-memory caching (24h TTL, project-isolated) is stated not to violate ZDR and is disableable per project. | Scope: **flagged traffic only** — logging happens only "if automated safety classifiers detect suspicious activity". Retention: **up to 90 days**, in the customer's selected region/multi-region. Access: authorised Google employees. Opt-out: request form, **"If approved"** → approval-gated. Separately, customers under a Google Cloud Master Agreement (rather than the online GCP ToS) are **exempt by default**. The 30-day full-logging Advanced AI Safety Addendum regime does **not** list any Gemini model. | **Google Cloud Service Specific Terms §18, §20(a), §20(h)** + **Google Cloud Platform Terms of Service §4.3** (both contract), with the abuse-monitoring docs page referenced by §4.3 | **Yes** | **Yes** |

### Load-bearing conclusions

1. **Google Vertex is now fully verified, and the open item from the companion doc §3.4 is closed.**
   The training and retention commitments are in the **contract**, not the documentation: Service
   Specific Terms §18 (Training Restriction), §20(a) (Generated Output is Customer Data), §20(h)
   (Handling of Prompts and Generated Output). The abuse carve-out is also contractual (GCP ToS §4.3)
   and — uniquely among the three — Google publishes a **number** for it (90 days) and a scope limit
   (flagged traffic only). Google is the strongest-documented of the three counterparties.

2. **AWS Bedrock is pinnable and is the strongest of the three for our two Claude routes,** but for a
   subtle reason worth recording accurately: **AWS never affirmatively promises "we will not train on
   Bedrock content."** The structure is (a) Service Terms §50.3 grants AWS a service-improvement right
   over an **enumerated list** of AI Services and says the section "does not apply to … any AI Service
   that is not listed", and Bedrock is not listed; plus (b) AWS Customer Agreement §1.4: "We will not
   access or use Your Content except as necessary to maintain or provide the Services." That is an
   absence-of-grant plus a general use limitation, which is materially different from an express
   no-training covenant. The express no-training covenant on this route comes from **Anthropic**, via
   an EULA that Service Terms §50.12.1 incorporates by reference.

3. **The issue-#294 framing — "pinning a cloud endpoint substitutes the cloud vendor's terms for the
   model vendor's" — is right in direction but wrong in exclusivity for Bedrock, and inverted for
   Azure+Claude.** On Bedrock the model vendor's terms are *added*, not replaced: §50.12.1 makes the
   Anthropic-on-Bedrock Commercial ToS binding alongside AWS's. On Azure, Claude is **not** a Microsoft
   product at all, so Microsoft's terms are largely *replaced by* Anthropic's — the cloud vendor
   contributes infrastructure and billing but no data commitment over prompts.

4. **`anthropic/claude-sonnet-5` → `azure` is not pinnable under the posture.** Microsoft's own
   documentation states that Claude models in Foundry "are third-party Marketplace offerings from
   Anthropic", that "Anthropic is the seller and operator … and acts as an independent data processor
   for prompts and outputs", and the partner-models page states plainly that such models "are
   Non-Microsoft Products under the Product Terms" — for which the Universal License Terms say
   "Customer's use of any Non-Microsoft Product shall be governed by the license, service, and/or
   privacy terms between Customer and the publisher" and "Microsoft … assumes no responsibility or
   liability whatsoever". Consequently the Microsoft Generative AI Services no-training clause does not
   reach this route. Anthropic's own no-training covenant does reach it — but **no retention period is
   stated in any operative document**, and there is a second unresolved variable: Foundry offers Claude
   in two hosting modes ("Hosted on Azure" and "Hosted on Anthropic infrastructure"), and in the latter
   "Data might be processed outside Azure, including outside the selected Azure region." **No primary
   source establishes which mode OpenRouter's `azure` endpoint for `anthropic/claude-sonnet-5` uses.**
   Pinning it would mean pinning to a counterparty whose processing location we cannot determine.
   Prefer `anthropic/claude-sonnet-5` → `amazon-bedrock`, which is fully verified.

5. **Azure's abuse-monitoring retention window is not published anywhere — contract or documentation.**
   The contract says "temporarily store"; the abuse-monitoring documentation page describes the data
   store, the classifiers, the SAW/JIT access controls, and the EEA personnel constraint, but states no
   duration. A targeted search of the fetched Azure pages for "30 days" / "thirty days" returned
   **nothing**. Any belief that Azure retains abuse-monitoring data for 30 days is **not supported by
   the current primary sources**. (For contrast, OpenAI's *first-party* API docs do state 30 days —
   companion doc §3.4 — but that is a different counterparty from Azure.)

6. **Every abuse-monitoring opt-out available on these three clouds is either approval-gated or
   scoped to an account we do not own.** Azure's modified abuse monitoring requires a Limited Access
   registration, Microsoft approval, and account-team management, and applies **per Azure
   subscription**. Google's abuse-logging exception requires an approved form and applies per Google
   Cloud account. AWS's `data_retention_mode: "none"` is self-service, but is set per AWS
   **account/project**. In all three cases the account is **OpenRouter's**. A control that requires
   approval is not available at pin time by default, and a control scoped to someone else's account is
   not ours to exercise at all.

7. **Nothing here is a direct contract between this repo and any cloud vendor.** See "Interaction with
   OpenRouter" below. Every commitment quoted runs from the cloud vendor to *its customer*, which is
   OpenRouter. Our assurance is transitive.

---

## 1. AWS Bedrock

Routes: `anthropic/claude-haiku-4.5` → `amazon-bedrock`; `anthropic/claude-sonnet-5` → `amazon-bedrock`.

### 1.1 Training on inputs and outputs

**Contract.** Amazon Bedrock is inside the "AI Services" definition, and outputs are Customer content:

> "'AI Services' means, collectively, Amazon Bedrock, Amazon CodeGuru Profiler, … 'AI Content' means
> Your Content that is processed by an AI Service."
> — AWS Service Terms §50.1, `https://aws.amazon.com/service-terms/` (Last Updated July 29, 2026; fetched 2026-08-12)

> "The output that you generate using AI Services is Your Content."
> — AWS Service Terms §50.2, same source

So both inputs and outputs are "Your Content". The service-improvement grant then **excludes**
Bedrock by enumeration:

> "You agree and instruct that for Amazon CodeGuru Profiler, Amazon Comprehend, Amazon Lex, Amazon
> Polly, Amazon Rekognition, Amazon Textract, Amazon Transcribe, Amazon Translate, AWS Transform, AWS
> FinOps Agent (Preview), Kiro Free Tier, and Kiro individual subscribers …: (a) we may use and store
> AI Content that is processed by each of the foregoing AI Services to develop and improve the
> applicable AI Service and its underlying technologies; (b) we may use and store AI Content that is
> not personal data to develop and improve AWS and affiliate machine-learning and artificial-intelligence
> technologies … **This Section does not apply to** Amazon Comprehend Medical, Amazon Transcribe
> Medical, AWS HealthScribe, Amazon Comprehend Detect PII **or any AI Service that is not listed in
> the first sentence of this Section 50.3.**"
> — AWS Service Terms §50.3, same source (emphasis added)

Amazon Bedrock is not in that first sentence. The positive obligation that fills the gap sits in the
customer agreement:

> "We will not access or use Your Content except as necessary to maintain or provide the Services, or
> as necessary to comply with the law or a binding order of a governmental body. We will not (a)
> disclose Your Content to any government or third party or (b) move Your Content from the AWS regions
> selected by you; except in each case as necessary to comply with the law or a binding order of a
> governmental body."
> — AWS Customer Agreement §1.4 (Data Privacy), `https://aws.amazon.com/agreement/` (Last Updated June 01, 2026; fetched 2026-08-12)

**Be precise about what this is.** §1.4 is a general use limitation, not a training covenant. It is
strong — training a foundation model is plainly not "necessary to maintain or provide the Services" —
but it is an inference from a general clause, not an express "AWS will not train on your Bedrock
prompts" sentence. No such express sentence exists in the AWS contract.

**Does the model vendor see or train on the traffic?** Two separate answers.

*Contract.* Sharing with the model provider is limited, and Anthropic-specific sharing is opt-in:

> "We may share information, that does not include Your Content, about your use of a third-party model
> with the provider of that third-party model."
> — AWS Service Terms §50.12.5

> "Certain Anthropic models identified on the Bedrock abuse detection page **require you to consent**
> to the transfer of Your Content and associated metadata to Anthropic for abuse detection, via the
> opt-in mechanism described in the applicable service documentation."
> — AWS Service Terms §50.12.2.2 (emphasis added)

Because the transfer is opt-in and gated on a page that lists neither Haiku 4.5 nor Sonnet 5 (§1.2
below), the default for our two models is no transfer to Anthropic.

*Anthropic's own covenant, incorporated by reference.* §50.12.1 says:

> "Third-party models are available to you on Amazon Bedrock as 'Third-Party Content'. By using a
> third-party model, you agree to the applicable terms here."
> — AWS Service Terms §50.12.1, where "here" resolves to `https://aws.amazon.com/legal/bedrock/third-party-models/`

That page carries, under the heading "Anthropic on Bedrock – Commercial Terms of Service":

> "Anthropic serverless models on Amazon Bedrock are sold by Anthropic. If you use any of these models
> on Amazon Bedrock, you agree to the seller's end user license agreement below."

> "B. Customer Content. … Anthropic disclaims any rights it receives to the Customer Content under
> these Terms. … **Anthropic may not train models on Customer Content from Services.** 'Inputs' means
> submissions to the Services by Customer or its Users and 'Outputs' means responses generated by the
> Services to Inputs (Inputs and Outputs together are 'Customer Content')."
> — Anthropic on Bedrock Commercial Terms of Service §B, `https://aws.amazon.com/legal/bedrock/third-party-models/` (fetched 2026-08-12, emphasis added)

This is the strongest single sentence available on any of the three clouds: an express no-training
covenant from the model vendor, covering inputs and outputs explicitly, reachable through the AWS
contract. It is unconditional on its face — no carve-out for abuse, safety, or aggregate use.

**Documentation only.** The claim that Anthropic cannot see Bedrock traffic at all is documentation:

> "Amazon Bedrock has a concept of a Model Deployment Account… These accounts are owned and operated
> by the Amazon Bedrock service team. Model providers don't have any access to those accounts. …
> Because the model providers don't have access to those accounts, they don't have access to Amazon
> Bedrock logs or to customer prompts and completions."
> — Amazon Bedrock User Guide, Data protection, `https://docs.aws.amazon.com/bedrock/latest/userguide/data-protection.html` (fetched 2026-08-12)

> "No, AWS and the third-party model providers will not use any inputs to or outputs from Amazon
> Bedrock to train Amazon Nova, Amazon Titan, or any third-party models."
> — Amazon Bedrock FAQs, Security, `https://aws.amazon.com/bedrock/faqs/` (fetched 2026-08-12)

> "Users' inputs and model outputs are not shared with any model providers."
> — same FAQ

These are the sentences people usually cite. **They are not contractual.** They corroborate the
contract; they are not a substitute for it. Record them as supporting evidence, not as the basis of
the pin.

### 1.2 Retention and abuse detection

**Contract.**

> "**Abuse Detection.** For certain models identified on the Bedrock abuse detection page, as part of
> providing the Service, Amazon Bedrock stores Service inputs and outputs for up to 30 days (unless
> otherwise required by law) solely to detect activity that violates our, or third-party model
> providers', terms of service or use policies. If we detect a potential violation, you agree and
> instruct that we may review the Service inputs and outputs to determine if a violation has occurred."
> — AWS Service Terms §50.12.2

Note the scoping: **"for certain models identified on the Bedrock abuse detection page"**. The
contract delegates the scope to a docs page — which makes that page **documentation,
contract-referenced**, and makes its model list operative.

**Documentation, contract-referenced.**

> "Amazon Bedrock uses a zero operator access (ZOA) data security model. This means no operators of
> the service can access model input or output. Also, Amazon Bedrock uses a zero data retention (ZDR)
> data security model. This means that **by default, Amazon Bedrock does not store model inputs or
> outputs.**
> However, for specific abuse detection purposes related to the following models, we may be required
> to store inputs and outputs:
> - For OpenAI GPT-5.4, GPT-5.5, GPT-5.6 Sol, GPT-5.6 Terra, and GPT-5.6 Luna, classifier-flagged
>   traffic will be retained for up to 30 days for automated offline abuse detection.
> - For Anthropic Claude Fable 5, inputs and outputs will be retained for up to 30 days. In order to
>   use Claude Fable 5, as required by Anthropic, you must opt in to sharing retained traffic with
>   Anthropic for abuse detection and potential human review.
>
> Retained inputs and outputs are stored and processed by AWS and are not shared with third-party
> model providers, unless you opt in to sharing with the model provider. … For these models, eligible
> customers may request full ZDR through their AWS account team."
> — Amazon Bedrock abuse detection, `https://docs.aws.amazon.com/bedrock/latest/userguide/abuse-detection.html` (fetched 2026-08-12, emphasis added)

**Neither `claude-haiku-4.5` nor `claude-sonnet-5` appears on that list.** For our two Bedrock routes,
the documented default is zero retention with no operator access, and §50.12.2's 30-day storage
authorisation does not attach.

The retention control itself is documented as a first-class API setting:

> "Amazon Bedrock gives you explicit control over whether your prompts and outputs are retained from
> your inference requests. You can configure data retention at the account or project level… If your
> account or project is configured for zero data retention (`data_retention_mode: none`) and you invoke
> a model that requires retention, Amazon Bedrock will block the request and return an error."

> "**Important** — There is no data retention change to Claude models released before Claude Fable 5."

> "`none` — Zero data retention. No request or response data is written to durable storage by AWS or
> shared with the model provider. … Chat Completions and Messages requests are never retained."
> — Amazon Bedrock Data retention, `https://docs.aws.amazon.com/bedrock/latest/userguide/data-retention.html` (fetched 2026-08-12)

**Is opting out approval-gated?** Two distinct answers, and the distinction matters:

- Setting `data_retention_mode: "none"` at account or project scope is **self-service** — a `PUT
  /v1/data_retention` API call, enforceable org-wide via SCP with the `DataRetentionMode` condition
  key. No approval.
- Obtaining ZDR **for a model that requires retention** *is* approval-gated: "If your organization
  requires zero data retention … contact your AWS account manager to discuss eligibility. ZDR access
  is evaluated on a per-account, per-model basis in coordination with the model provider."

Our two models do not require retention, so we land in the first case — but the setting lives on
**OpenRouter's** AWS account or project, not ours (§4).

### 1.3 Verdict

**Verifiable: yes. Pinnable: yes.** Training is covered by an express model-vendor covenant reachable
through the AWS contract, plus an absence-of-grant and a general use limitation from AWS itself.
Retention is zero by default for both admissible models, and the contract's 30-day abuse-storage
authorisation is scoped to a model list that excludes them. The residual weaknesses are (a) AWS's own
no-training position is structural rather than express, and (b) the model list that scopes §50.12.2 is
a docs page AWS can edit without a terms-change notice — so the pin record must snapshot the list, and
re-verification must re-read it.

---

## 2. Microsoft Azure

Two admissible routes with **different operative counterparties**. They must be assessed separately.

### 2.1 Which models Microsoft's own terms actually cover

> "This article lists a selection of Foundry Models sold by Azure… Models sold by Azure are also hosted
> by Azure and operated by Azure as part of the Foundry Models service. They include all Azure OpenAI
> models and specific, selected models from top providers."
> …
> "GPT-5.6 series **NEW** — `gpt-5.6-sol`, `gpt-5.6-terra`, `gpt-5.6-luna`"
> — Foundry Models sold by Azure, `https://learn.microsoft.com/en-us/azure/foundry/foundry-models/concepts/models-sold-directly-by-azure` (fetched 2026-08-12)

So **`gpt-5.6-luna` is a Foundry Model sold by Azure**. Claude is not:

> "**Important** — Models from partners and community that are not sold by Azure are Non-Microsoft
> Products under the Product Terms."
> …
> "**Anthropic** — Anthropic's flagship product is Claude… Microsoft Foundry offers Claude models in
> two versions: Hosted on Azure and Hosted on Anthropic infrastructure deployments."
> (the table lists `claude-sonnet-5`, `claude-haiku-4-5`, `claude-opus-5`, `claude-fable-5`, …)
> — Foundry Models from partners and community, `https://learn.microsoft.com/en-us/azure/foundry/foundry-models/concepts/models-from-partners` (fetched 2026-08-12)

And the Product Terms confirm the consequence:

> "**Third-party models.** Any third-party models that Microsoft makes available through Microsoft
> Foundry Models (including in a Model Catalog, Model Registry, or otherwise), but which are not
> Foundry Models sold by Azure or First-Party Consumption Services, are Non-Microsoft Products and
> subject to the terms for Non-Microsoft Products."
> — Microsoft Product Terms, Microsoft Azure product offering terms → Microsoft Foundry Models,
> `https://www.microsoft.com/licensing/terms/productoffering/MicrosoftAzure/EAEAS` (effective 8/10/2026; fetched 2026-08-12)

> "Microsoft … assumes no responsibility or liability whatsoever for any Non-Microsoft Product. …
> Customer's use of any Non-Microsoft Product shall be governed by the license, service, and/or privacy
> terms between Customer and the publisher of the Non-Microsoft Product (if any)."
> — Microsoft Product Terms, Universal License Terms for Online Services → Other Non-Microsoft Products,
> `https://www.microsoft.com/licensing/terms/product/ForOnlineServices/all` (effective 8/10/2026; fetched 2026-08-12)

### 2.2 `openai/gpt-5.6-luna` → `azure`

**Training — contract.**

> "**Use of Content for Training.** *By Microsoft.* Microsoft Generative AI Services will not use
> Customer Data to train any generative AI foundation model, except pursuant to Customer's documented
> instructions."
> …
> "**Output Content.** Output Content is Customer Data. Microsoft does not own Customer's Output
> Content."
> — Microsoft Product Terms, Universal License Terms for Online Services → Microsoft Generative AI Services,
> `https://www.microsoft.com/licensing/terms/product/ForOnlineServices/all` (effective 8/10/2026; fetched 2026-08-12)

Coverage: inputs **and** outputs, because Output Content is defined into Customer Data. The
carve-out — "except pursuant to Customer's documented instructions" — is the standard processor
formulation and is not triggered by ordinary inference use.

**Limits of that clause.** It binds *Microsoft*. It says **nothing** about whether OpenAI may train
on the traffic. The only statement to that effect is documentation:

> "Your prompts (inputs) and completions (outputs), your embeddings, and your training data:
> are NOT available to other customers. are NOT available to OpenAI or other providers of Models sold
> by Azure. are NOT used by providers of Models sold by Azure to improve their models or services.
> are NOT used to train any generative AI foundation models without your permission or instruction."
> …
> "Models sold by Azure do NOT interact with any services operated by providers of Models sold by
> Azure, for example, OpenAI (e.g. ChatGPT, or the OpenAI API)."
> — Data, privacy, and security for Models sold by Azure in Microsoft Foundry,
> `https://learn.microsoft.com/en-us/azure/foundry/responsible-ai/openai/data-privacy` (updated 2026-05-19; fetched 2026-08-12)

That page is **documentation only** — the Product Terms do not incorporate it. It is the clearest
statement of the "OpenAI never sees it" claim and it has no contractual hook. Record it as such.

> "The models are stateless: no prompts or completions are stored in the model. Additionally, prompts
> and completions are not used to train, retrain, or improve the base models."
> — same page

**Retention and abuse monitoring — contract.**

> "**Data Use and Access for Abuse Monitoring**: Except for the Limited exception below, as part of
> providing the Foundry Models sold by Azure, Microsoft will **temporarily store** Input and Output
> Content, to monitor for and prevent abusive or harmful uses or outputs of the service. Authorized
> Microsoft employees may review such data that has triggered our automated systems to investigate and
> verify potential abuse. For customers who have deployed Foundry Models sold by Azure in the EU Data
> Boundary, the authorized Microsoft employees will be located in the European Economic Area. …
> **Limited exception.** The Data Use and Access for Abuse Monitoring terms will not apply if and to
> the extent Customer is approved for and complies with all requirements to use Foundry Models sold by
> Azure with Modified Abuse Monitoring."
> — Microsoft Product Terms, Azure product offering terms → Microsoft Foundry Models (effective 8/10/2026; fetched 2026-08-12, emphasis added)

**"Temporarily" is the entire retention commitment. No duration is stated.** And the documentation
does not supply one either — the abuse-monitoring page describes the mechanism in detail but gives no
number:

> "By default, if prompts and completions are flagged through content classification as harmful and/or
> identified to be part of a potentially abusive pattern of use, they might be sampled for review by
> using automated means including AI models such as LLMs instead of a human reviewer. … prompts and
> completions that undergo such review are not stored by the abuse monitoring system or used to train
> the AI model or other systems."
> …
> "Such prompts and completions can be accessed for human review only by authorized Microsoft employees
> via Secure Access Workstations (SAWs) with Just-In-Time (JIT) request approval granted by team
> managers."
> — Abuse monitoring, `https://learn.microsoft.com/en-us/azure/foundry/openai/concepts/abuse-monitoring` (updated 2026-05-19; fetched 2026-08-12)

> "The abuse monitoring data store where prompts and completions are stored for human review is
> logically separated by customer resource… a customer's prompts and generated content are stored in
> the Azure geography where the customer's Foundry resource is deployed, within the Models sold by
> Azure service boundary."
> — Data, privacy, and security for Models sold by Azure (updated 2026-05-19; fetched 2026-08-12)

A targeted search of both pages and of the Product Terms for "30 days" / "thirty days" returned
nothing. **The Azure abuse-monitoring retention window is not published.**

**Opt-out is approval-gated and per-subscription.**

> "Microsoft allows customers who meet additional Limited Access eligibility criteria to apply to
> modify abuse monitoring by completing this form. Some advanced Models sold by Azure may have more
> stringent criteria for turning off abuse monitoring."
> — Abuse monitoring (same source)

> "At this time, modified Guardrails (previously content filters) and/or modified abuse monitoring for
> Models sold by Azure are available only to customers and partners **managed by a Microsoft account
> team or under an eligible program**, and are subject to additional requirements."
> — Limited access for Foundry Models sold by Azure, `https://learn.microsoft.com/en-us/azure/foundry/responsible-ai/openai/limited-access` (updated 2026-05-19; fetched 2026-08-12, emphasis added)

The Product Terms make Limited Access a contractual regime with re-verification duties (respond within
10 business days; supply additional information within 30 business days) and a Microsoft right to
re-assess eligibility. Verification that it is on is per-resource:

> "There will be a value in the Capabilities list called 'ContentLogging' which will appear and be set
> to FALSE when logging for abuse monitoring is off."
> — Data, privacy, and security for Models sold by Azure (same source)

That check runs against **an Azure subscription and Foundry resource** — OpenRouter's, not ours (§4).

**Verdict.** Training: **verified** (contract). Retention: **not verified** — the operative contract
says only "temporarily store" and no primary source anywhere gives a duration. Abuse-monitoring
opt-out exists but requires Microsoft approval plus account-team management, so it is **not available
at pin time by default**, and is in any case scoped to OpenRouter's subscription. Pinnable only with
the undisclosed retention window recorded as an accepted residual risk on the pin.

### 2.3 `anthropic/claude-sonnet-5` → `azure`

**The operative counterparty is Anthropic, not Microsoft.**

> "Claude models in Microsoft Foundry are third-party Marketplace offerings from Anthropic. Data
> handling depends on the hosting option you select when deploying the Claude model. … For both hosting
> options, **Anthropic is the seller and operator** of Claude models in Microsoft Foundry and **acts as
> an independent data processor for prompts and outputs** associated with Claude models. Your use of the
> Claude models is subject to the terms of use Anthropic provides for Claude models and APIs."
> — Data, privacy, and security for Claude models in Microsoft Foundry,
> `https://learn.microsoft.com/en-us/azure/foundry/responsible-ai/claude-models/data-privacy` (updated 2026-06-29; fetched 2026-08-12, emphasis added)

The hosting-comparison table is explicit that Microsoft supplies no retention term of its own:

| Topic | Hosted on Azure | Hosted on Anthropic |
| --- | --- | --- |
| Seller of record | Anthropic | Anthropic |
| Data processor | Anthropic | Anthropic |
| **Data retention** | Governed by Anthropic's Data Processing Addendum and Anthropic's Commercial Terms of Service | Governed by Anthropic's Data Processing Addendum and Anthropic's Commercial Terms of Service |
| Data residency | "Data at rest is stored in the selected Azure geography…" | "Data might be processed outside Azure, including outside the selected Azure region." |

— Compare hosting options for Claude models in Microsoft Foundry,
`https://learn.microsoft.com/en-us/azure/foundry/foundry-models/concepts/claude-models-hosting-comparison` (fetched 2026-08-12)

What Microsoft *does* commit to on this route is narrow:

> "Microsoft continues to provide Microsoft Foundry experience, Azure infrastructure, and billing
> services… Microsoft also collects billing, usage, customer contact, and transaction information for
> Marketplace operations. Microsoft might share such customer contact information, transaction details,
> and usage information with Anthropic… Microsoft processes data **for these services** under the
> Microsoft Products and Services Data Protection Addendum and applicable Marketplace terms."
> — Data, privacy, and security for Claude models in Microsoft Foundry (emphasis added)

i.e. the Microsoft DPA covers Marketplace/billing data on this route, not prompt handling. (The
Microsoft DPA, May 2026 WW English edition, was downloaded and searched: it contains no AI-training
clause and its "Data Retention and Deletion" section addresses post-termination retention of stored
Customer Data — 90 days extraction window, deletion within a further 90 days — not inference logs. Its
general use limitation is "When providing Products and Services, Microsoft will not use or otherwise
process Customer Data … for: (a) user profiling, (b) advertising or similar commercial purposes, or
(c) market research aimed at creating new functionalities, services, or products or any other purpose,
unless such use or processing is in accordance with Customer's documented instructions.")

**Training.** Anthropic's covenant applies, from the same Commercial ToS quoted in §1.1:
"Anthropic may not train models on Customer Content from Services", with Inputs and Outputs both
inside "Customer Content"
(`https://www.anthropic.com/legal/commercial-terms`, fetched 2026-08-12). Verified.

**Retention.** Not verified. Microsoft points at Anthropic's ToS and DPA; **neither states a retention
period**. The Anthropic Commercial ToS contains no retention duration for Inputs or Outputs. The
Anthropic DPA's only retention text is a post-termination deletion obligation with carve-outs:

> "delete all copies of Customer Data (including Customer Personal Data) processed by Anthropic or any
> Subprocessors, except to the extent (i) Applicable Data Protection Laws or other applicable legal or
> regulatory requirements requires storage of the Customer Data, (ii) retention of the Customer Data by
> Anthropic is necessary to resolve a dispute between the parties, or (iii) **retention of the Customer
> Data is necessary to combat harmful use of the Services**."
> — Anthropic Data Processing Addendum §H.1.b, `https://www.anthropic.com/legal/data-processing-addendum` (fetched 2026-08-12, emphasis added)

Carve-out (iii) is exactly the trust-and-safety retention we would need a number for, and there is no
number.

**Abuse monitoring.** Described only in documentation, without duration, access controls, or opt-out:

> "Automatic safeguards flag content that might be sent to Anthropic Trust & Safety for review.
> Anthropic personnel review customer content on an exceptions-only basis to investigate potential
> safety violations, subject to applicable Anthropic terms."
> — Data, privacy, and security for Claude models in Microsoft Foundry (updated 2026-06-29)

**Verdict: not pinnable.** Two independent blockers. (1) No retention period is stated in any operative
document — Microsoft delegates to Anthropic and Anthropic is silent. (2) The route's processing
location is indeterminate from primary sources: Foundry offers Claude "Hosted on Azure" and "Hosted on
Anthropic infrastructure", the latter processing data outside Azure and outside the selected region,
and **nothing in Microsoft's or Anthropic's documents establishes which one OpenRouter's `azure`
endpoint for `anthropic/claude-sonnet-5` resolves to**. Use `amazon-bedrock` for `claude-sonnet-5`,
which is fully verified.

---

## 3. Google Vertex AI

Route: `google/gemini-3.5-flash-lite` → `google-vertex`. Google is both cloud vendor and model vendor
here, so the counterparty-substitution problem from issue #294 does not arise on this route.

**This section closes the UNVERIFIED item recorded in the companion doc §3.4.** The statements were
not missing; the docs set moved (see "Redirects" above).

### 3.1 Training on inputs and outputs

**Contract.**

> "**18. Training Restriction** (formerly Section 17 (Training Restriction)). Google will not use
> Customer Data to train or fine-tune any AI/ML models without Customer's prior permission or
> instruction."
> — Google Cloud Service Specific Terms §18, `https://cloud.google.com/terms/service-terms` (fetched 2026-08-12)

Outputs are inside "Customer Data" by express definition:

> "**20. Generative AI Services** … *a. Definition.* 'Generated Output' means the data or content
> generated by a Generative AI Service prompted by Customer Data. **Generated Output is Customer Data.**
> As between Customer and Google, Google does not assert any ownership rights in any new intellectual
> property created in the Generated Output."
> — Google Cloud Service Specific Terms §20(a), same source (emphasis added)

So §18 covers inputs and outputs. The carve-out is "without Customer's prior permission or
instruction" — the standard processor formulation, not triggered by ordinary inference use.

Documentation restates the same in scope terms, and cites the contract as its authority:

> "**Training restriction** — As outlined in 'Training Restriction' in the Service Terms section of the
> Service Specific Terms, Google won't use your data to train or fine-tune any AI/ML models without your
> prior permission or instruction. This applies to all managed models on Gemini Enterprise Agent
> Platform, including GA and pre-GA models."
> — Gemini Enterprise Agent Platform and zero data retention,
> `https://docs.cloud.google.com/gemini-enterprise-agent-platform/resources/zero-data-retention` (fetched 2026-08-12)

### 3.2 Retention

**Contract.**

> "*h. Handling of Prompts and Generated Output.* Absent Customer's prior permission or instruction,
> Google will not store outside Customer's Account (i) Customer Data prompted to a Generative AI Service
> for longer than is reasonably necessary to create the Generated Output, or (ii) the Generated Output."
> — Google Cloud Service Specific Terms §20(h), same source

This is a genuine contractual retention commitment, and it is the only one of the three clouds that
states a retention *rule* (rather than a duration or nothing). Note its shape: it bars storage
**outside Customer's Account**, and it is qualified by the abuse carve-out in §4.3 of the GCP ToS,
which explicitly overrides it.

Documentation enumerates every mechanism by which data can nevertheless persist, and what to do about
each; the ones that matter for a plain inference pin:

- **Request-response logging** — "This feature is disabled by default… To achieve zero data retention,
  do not enable this feature."
- **Interactions API `store`** — defaults to `true`; irrelevant to a Chat-Completions-shaped call, but
  "To achieve zero data retention, explicitly set `store = false`".
- **In-memory caching** — "By default, Google's published Gemini models cache Customer Data (inputs,
  outputs, and derived data) in-memory to reduce latency… This data is stored only in-memory (not
  at-rest), is isolated at the project level, and has a 24-hour TTL. Cached data is used only for
  improving service performance… and **does not violate zero data retention**. This feature can be
  disabled at the project level." (Consistent with OpenRouter's own ZDR page, which likewise says
  in-memory caching is not considered data retention — companion doc §3.2.)
- **Grounding with Google Search / Maps** — retention of 3 days / 30 days respectively, with "no way to
  disable"; **not used by this repo's Language Layer**, but a hard constraint if grounding tools were
  ever enabled.

— all from the zero-data-retention page, fetched 2026-08-12.

### 3.3 Abuse monitoring

**Contract.**

> "**4.3 Generative AI Safety and Abuse for GCP Services.** Google uses automated safety tools to detect
> abuse of Generative AI Services. Notwithstanding the 'Handling of Prompts and Generated Output'
> section in the Service Specific Terms for GCP Services, if these tools detect potential abuse or
> violations of Google's AUP or Prohibited Use Policy, Google may log Customer prompts solely for the
> purpose of reviewing and determining whether a violation has occurred. See the Abuse Monitoring
> documentation page for more information about how logging prompts impacts Customer's use of the GCP
> Services."
> — Google Cloud Platform Terms of Service §4.3, `https://cloud.google.com/terms/` (fetched 2026-08-12)

Two things to notice: the clause is **conditional on detection** (not blanket storage), and it covers
**prompts**, not completions.

**Documentation, contract-referenced** (§4.3 points at this page):

> "**Prompt logging**: If automated safety classifiers detect suspicious activity that requires further
> investigation into whether a customer has violated our policies, then Google may log customer prompts
> solely for the purpose of examining whether a violation of the AUP or Prohibited Use Policy has
> occurred. **This data won't be used to train or fine-tune any AI/ML models. This data is stored
> securely for up to 90 days** in the same region or multi-region selected by the customer for their
> project and adheres to Google Cloud assurances, such as Data Residency, Access Transparency and VPC
> Service Controls. Prompt logs for the purposes of abuse monitoring are not encrypted by
> Customer-managed encryption keys (CMEK). Customers also have the option to request an opt-out from
> abuse logging."
> …
> "**Customers in scope**: Only customers whose use of Google Cloud is governed by the Google Cloud
> Platform Terms of Service. This means that customers with a Google Cloud Master Agreement are **exempt
> from prompt logging for this abuse monitoring by default**."
> …
> "**Customer opt-out**: Customers may request for an exception by filling out this form. **If
> approved**, Google won't store any prompts associated with the approved Google Cloud account."
> — Abuse monitoring, `https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/abuse-monitoring` (fetched 2026-08-12, emphasis added)

So: **90 days, flagged traffic only, in-region, no CMEK, opt-out is approval-gated**, and a customer on
a negotiated Master Agreement rather than the online ToS is out of scope by default. Which of those two
applies to OpenRouter is not knowable from Google's documents (§4).

**Advanced AI Safety Addendum — a stricter regime that does not reach our route.**

> "**Prompt and response logging**: All prompts and responses will be logged and securely stored for up
> to 30 days for the sole purpose of monitoring for abuse. This data won't be used to train or fine-tune
> any AI/ML models. … It may not be possible to opt-out of prompt-response logging when using some
> Advanced AI features."
> …
> "**Services in scope**: Models and features designated as 'Advanced AI', including:
> Claude Mythos (all versions); Claude Fable (all versions); Claude Sonnet >=5 and Claude Opus >=4.7
> when used for high risk dual use or prohibited use cases covered under Anthropic's Cyber Verification
> Program"
> — Abuse monitoring, same source

**No Gemini model appears on that list**, so `google/gemini-3.5-flash-lite` is not subject to the
30-day full-logging regime. Worth recording anyway: it is blanket logging of prompts *and* responses,
consent is per-project via Model Garden, and opting out may be impossible. Should the shortlist ever
reopen to Claude on Vertex, this clause is decisive.

### 3.4 Verdict

**Verifiable: yes. Pinnable: yes.** Training and retention are contractual (Service Specific Terms §18,
§20(a), §20(h)); the abuse carve-out is contractual (GCP ToS §4.3) with a documented, contract-referenced
90-day bound scoped to flagged traffic only; the strict 30-day Advanced-AI regime does not reach Gemini.
Residual weaknesses: the 90-day number and the flagged-only scoping live in documentation rather than in
§4.3 itself; opting out of abuse logging is approval-gated and account-scoped; and Google publishes no
current-version date on either terms page, so the pin record must cite fetch date plus quoted text.

---

## 4. Interaction with OpenRouter

**The customer of record is OpenRouter, not this repo.** Every clause quoted above runs from a cloud
vendor to *its* customer. AWS Customer Agreement §1.4 says "We will not access or use **Your** Content";
the Microsoft ULT says "Microsoft Generative AI Services will not use **Customer** Data"; Google's
Service Specific Terms §18 says "Google will not use **Customer** Data". In each case the Customer is
the entity holding the account that transmits the request — OpenRouter. This repo holds no privity with
AWS, Microsoft, or Google on any of the four admissible routes.

What follows from that, supported by the sources:

1. **Our assurance is transitive.** The cloud terms bind the cloud vendor toward OpenRouter's account.
   Whether that protection reaches us depends on OpenRouter's own contract with us — the OpenRouter
   privacy policy and ZDR/data-collection controls recorded in companion doc §3.1–3.2. The conjunction
   in companion doc §3.5 remains the correct construction; this document supplies its step 6 ("the
   pinned counterparty's own first-party terms read and recorded") for the three cloud counterparties.

2. **Every account-scoped control identified here is on OpenRouter's account, not ours.** This is
   explicit in the primary sources, not inferred:
   - Azure's modified abuse monitoring is verified via `az cognitiveservices account show` on the
     **Foundry resource** in an **Azure subscription** — OpenRouter's subscription. The Limited Access
     registration form, the eligibility criteria, and the re-verification duties all attach to that
     subscription holder.
   - Google's abuse-logging exception, if approved, means "Google won't store any prompts associated
     with the approved **Google Cloud account**" — OpenRouter's account.
   - AWS's `data_retention_mode` is set by `PUT /v1/data_retention` at **account** or **project** scope,
     and enforced by SCPs in **an AWS Organization** — OpenRouter's.

   We cannot set, verify, or audit any of these three from our side. OpenRouter's `/api/v1/providers`
   response exposes `privacy_policy_url` and `terms_of_service_url` per provider but nothing about the
   provider account's configuration (companion doc §3.2).

3. **Google's abuse-monitoring scope turns on which agreement OpenRouter signed.** The abuse page limits
   prompt logging to "customers whose use of Google Cloud is governed by the Google Cloud Platform Terms
   of Service" and states that customers with a Google Cloud Master Agreement are exempt by default.
   Whether OpenRouter is on the online ToS or a negotiated Master Agreement is **not determinable from
   any primary source available to us**. *Inference, marked as such:* a provider operating at
   OpenRouter's Vertex volume is more likely to be on a negotiated agreement, which would make the
   90-day prompt logging inapplicable — but this is an inference and must not be recorded as a fact.

4. **Bedrock's model-list gating is the one place where our pin choice does real work.** The Bedrock
   abuse-detection page ties 30-day storage to specific models. Because our two Claude pins are not on
   that list, the retention outcome does not depend on OpenRouter's account configuration at all — it
   depends on the model. That makes Bedrock the least account-coupled of the three counterparties, which
   is a genuine argument for preferring it. *Inference, marked as such:* if OpenRouter's AWS account were
   set to `provider_data_share`, our models would still not be shared, because the docs state each model's
   `allowed_modes` governs and "Most models currently do not require or request `provider_data_share`" —
   but we cannot observe OpenRouter's setting to confirm.

5. **Anything the cloud vendor logs about *usage* rather than content is out of scope of every clause
   above and is shared.** AWS ST §50.12.5: "We may share information, that does not include Your Content,
   about your use of a third-party model with the provider of that third-party model." Microsoft: "Microsoft
   might share such customer contact information, transaction details, and usage information with
   Anthropic." Both are about OpenRouter's account metadata, not our prompts, but both mean the model
   vendor learns that traffic exists.

---

## 5. What the pin record must cite

For every pin, record the model, the endpoint, the ZDR-list verification, **and** the counterparty
document set below — each with the resolving URL, the version marker, and the fetch date. A pin whose
counterparty documents were not re-fetched on the day the pin was set is not a verified pin.

### If the pin is `amazon-bedrock` (`claude-haiku-4.5` or `claude-sonnet-5`)

| Cite | URL | Version marker to record |
| --- | --- | --- |
| AWS Service Terms §50.1, §50.2, §50.3, §50.12.1, §50.12.2, §50.12.2.2, §50.12.5 | `https://aws.amazon.com/service-terms/` | the page's own "Last Updated" line (was **July 29, 2026**) |
| AWS Customer Agreement §1.4 (Data Privacy) | `https://aws.amazon.com/agreement/` | "Last Updated" line (was **June 01, 2026**) |
| Anthropic on Bedrock – Commercial Terms of Service §B (Customer Content) | `https://aws.amazon.com/legal/bedrock/third-party-models/` | no version date published → record fetch date + the quoted sentence |
| Amazon Bedrock abuse detection — **the model list** | `https://docs.aws.amazon.com/bedrock/latest/userguide/abuse-detection.html` | undated → record fetch date **and the verbatim model list**, since §50.12.2's scope depends on it |
| AWS DPA (incorporated by ST §1.14.1) | `https://d1.awsstatic.com/legal/aws-dpa/aws-dpa.pdf` | record fetch date |

Also record, as documentation-only corroboration and not as basis: the Bedrock ZOA/ZDR-by-default
sentence, the Model Deployment Account paragraph from the Data protection page, and the Bedrock FAQ
"AWS and the third-party model providers will not use any inputs to or outputs from Amazon Bedrock to
train … any third-party models."

### If the pin is `azure` with `openai/gpt-5.6-luna`

| Cite | URL | Version marker to record |
| --- | --- | --- |
| Microsoft Product Terms — Universal License Terms for Online Services, "Microsoft Generative AI Services" → Use of Content for Training; Output Content | `https://www.microsoft.com/licensing/terms/product/ForOnlineServices/all` | the effective date selected (was **August 10, 2026**) |
| Microsoft Product Terms — Azure product offering terms → Microsoft Foundry Models → Data Use and Access for Abuse Monitoring; Limited Access Services | `https://www.microsoft.com/licensing/terms/productoffering/MicrosoftAzure/EAEAS` | same effective date |
| Microsoft Products and Services DPA | `https://www.microsoft.com/licensing/docs/view/Microsoft-Products-and-Services-Data-Protection-Addendum-DPA` | edition label (was **May 2026 (WW) (English)**) |
| Foundry Models sold by Azure — proof that `gpt-5.6-luna` is in scope | `https://learn.microsoft.com/en-us/azure/foundry/foundry-models/concepts/models-sold-directly-by-azure` | fetch date + the model-list line |
| Data, privacy, and security for Models sold by Azure (documentation only) | `https://learn.microsoft.com/en-us/azure/foundry/responsible-ai/openai/data-privacy` | the page's "Last updated on" (was **2026-05-19**) |

Plus an explicit **negative finding** on the pin: *no retention duration for abuse-monitoring data is
published in the Product Terms or in the Foundry documentation as of the fetch date.*

### If the pin is `google-vertex` (`gemini-3.5-flash-lite`)

| Cite | URL | Version marker to record |
| --- | --- | --- |
| Google Cloud Service Specific Terms §18 (Training Restriction), §20(a) (Generated Output is Customer Data), §20(h) (Handling of Prompts and Generated Output) | `https://cloud.google.com/terms/service-terms` | **no current-version date is printed** → record fetch date + the quoted text; optionally note the newest archived version (was **April 22, 2026**) |
| Google Cloud Platform Terms of Service §4.3 (Generative AI Safety and Abuse for GCP Services) | `https://cloud.google.com/terms/` | no current-version date → fetch date + quoted text |
| Abuse monitoring — 90-day prompt logging, flagged-only scope, opt-out, Advanced AI scope list | `https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/abuse-monitoring` | undated → fetch date + the Advanced-AI "Services in scope" list |
| Gemini Enterprise Agent Platform and zero data retention | `https://docs.cloud.google.com/gemini-enterprise-agent-platform/resources/zero-data-retention` | undated → fetch date |
| Cloud Data Processing Addendum | `https://cloud.google.com/terms/data-processing-addendum` | fetch date |

Also record the **redirect chain**, because the historical URL is what a future reader will try:
`cloud.google.com/vertex-ai/generative-ai/docs/data-governance` →
`docs.cloud.google.com/gemini-enterprise-agent-platform/resources/zero-data-retention`.

### Do not set a pin on `anthropic/claude-sonnet-5` → `azure`

If it is set anyway, the record must cite Anthropic's Commercial ToS and DPA as the operative
documents (not Microsoft's), cite the Foundry Claude data-privacy and hosting-comparison pages as the
evidence that Microsoft supplies no prompt-retention term, and carry both blockers from §2.3 as
unresolved.

---

## Open questions / what could not be verified

Primary sources did **not** establish the following. Each is a real risk to the pin, not a formatting
gap.

1. **Azure's abuse-monitoring retention duration.** The Product Terms say "temporarily store Input and
   Output Content" with no period; the abuse-monitoring and data-privacy documentation pages describe the
   store, the access controls, and the geography but give **no number**. Searching both pages for "30
   days"/"thirty days" returned nothing. *What would be needed:* a Microsoft document stating the
   duration — most plausibly a revision of the abuse-monitoring page, a Foundry-specific supplement to
   the Product Terms, or an answer from a Microsoft account team. Until then, the `openai/gpt-5.6-luna` →
   `azure` pin carries an undisclosed retention window.
2. **Which Foundry hosting mode OpenRouter's `azure` endpoint uses for `anthropic/claude-sonnet-5`.**
   "Hosted on Azure" keeps data at rest in the selected Azure geography; "Hosted on Anthropic
   infrastructure" may process "outside Azure, including outside the selected Azure region". No primary
   source ties OpenRouter's endpoint to either. *What would be needed:* an OpenRouter statement of the
   deployment mode, or an endpoint-level field exposing it.
3. **Any retention period for Claude served through Azure.** Microsoft delegates to Anthropic's ToS and
   DPA; neither states one. The DPA's only relevant text is a post-termination deletion obligation with a
   carve-out for retention "necessary to combat harmful use of the Services", which is precisely the
   trust-and-safety retention we need bounded. *What would be needed:* an Anthropic document stating the
   retention window for enterprise/API traffic, incorporated by or referenced from the Commercial ToS.
4. **Whether OpenRouter is on the Google Cloud Platform Terms of Service or a Google Cloud Master
   Agreement.** This decides whether Google's 90-day prompt logging applies to our Vertex traffic at all,
   since Master Agreement customers are exempt by default. Not determinable from any Google document.
   *What would be needed:* a statement from OpenRouter.
5. **Whether AWS, Microsoft, or Google have granted OpenRouter any account-level exemption.** AWS
   `data_retention_mode`, Azure modified abuse monitoring, and Google's abuse-logging exception are all
   configured on OpenRouter's account and are unobservable from our side. OpenRouter's `/api/v1/providers`
   exposes no such field. *What would be needed:* an OpenRouter attestation, or a per-endpoint field in
   the ZDR/providers API.
6. **AWS's express position on training.** AWS never says "we will not train on Bedrock content." The
   protection is the absence of a §50.3 grant plus AWS Customer Agreement §1.4's general use limitation,
   corroborated by an FAQ. This is strong but structural. *What would be needed:* an express covenant in
   the Service Terms, or acceptance that the structural reading is sufficient — which should be an
   explicit, recorded decision rather than an assumption.
7. **Whether Microsoft's "not available to OpenAI" claim has any contractual basis.** It appears only on
   the data-privacy documentation page. The Product Terms bind Microsoft and are silent on the model
   provider. *What would be needed:* a Product Terms clause, or acceptance of documentation-grade evidence
   for that specific claim.
8. **Google's current terms version.** Neither `cloud.google.com/terms/` nor
   `cloud.google.com/terms/service-terms` prints a version date for the live document; only an archive
   list is published. Pin records must therefore rely on fetch date plus quoted text, which makes
   detecting a silent amendment harder. *What would be needed:* a dated version marker on the live page,
   or a periodic diff against the archive.
9. **The AWS DPA body.** The DPA is incorporated into the Service Terms by §1.14.1 and its URL was
   confirmed to resolve, but the PDF body was not parsed for this document. It is unlikely to contradict
   §50 or Customer Agreement §1.4, but that is an assumption, not a finding.
10. **Stability of the Bedrock abuse-detection model list.** AWS Service Terms §50.12.2 delegates its
    entire scope to a docs page. That page can be edited without a terms-change notice, and adding
    `claude-haiku-4.5` or `claude-sonnet-5` to it would silently switch on 30-day storage for a live pin.
    There is no documented change-notification mechanism for that page. **This is the single most likely
    way a verified Bedrock pin becomes stale without anyone noticing**, and it argues for re-fetching that
    page on a schedule, not only at pin time.
