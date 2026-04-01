# RUSVEL Market Research — 2026-04-01

> Research sprint findings: freelancer platforms, content machine, MCP ecosystem, competing products.
> Goal: ground truth for designing 3 core workflows (find work, build brand, win clients).

---

## 1. Freelancer Platform APIs

### Platform Summary

| Platform | Job Search API | Proposal/Bid Submission | RSS | TOS on Automation |
|---|---|---|---|---|
| **Upwork** | GraphQL `marketplaceJobPostingsSearch` (filters unreliable) | **Prohibited by TOS** | **Dead** (Aug 2024) | Discovery OK; submission banned |
| **Freelancer.com** | REST v1, well-documented | **API POST supported** | Live | Gray area; approval-gate recommended |
| **Toptal** | None (matching model) | N/A | None | N/A |
| **LinkedIn** | Partner-only, not accessible | Not possible | Third-party only (RSS.app) | Scraping prohibited |
| **PPH / Guru** | None public | None public | Live | Monitor-only safe |
| **Fiverr** | None (seller-side) | N/A | None | Skip — wrong model |
| **Contra / Arc / Gun.io** | None | N/A (curated matching) | None | N/A |

### Critical Finding: RUSVEL's UpworkRssSource is Broken

Upwork killed RSS feeds August 2024. The `UpworkRssSource` in `harvest-engine/src/source.rs` hits a dead URL. Must replace with:
- **Option A:** Upwork GraphQL `marketplaceJobPostingsSearch` via OAuth (preferred)
- **Option B:** Parse Upwork email notifications via Gmail MCP (ToS-compliant workaround)

### Freelancer.com — Best Automation Target

- Developer portal: `https://developers.freelancer.com/`
- Auth: OAuth 2.0 with custom header `freelancer-oauth-v1: <token>`
- Search: `GET /api/projects/0.1/projects/` — filter by query, skills, budget
- **Bid submission**: `POST /api/projects/0.1/bids/` — amount, period, description
- RSS still live: `https://www.freelancer.com/rss/projects?query=...`
- Rate limit: ~60 req/min

### Realistic Automation Flow

```
DISCOVER (every 10-30 min)
  → Freelancer.com API + RSS
  → PPH/Guru RSS
  → Upwork GraphQL (OAuth) or email notifications via Gmail MCP
  → LinkedIn via RSS.app feed

DEDUPLICATE (content_hash + platform_job_key — already in harvest-engine)

SCORE (harvest-engine scorer — LLM + keyword dual method)

GENERATE PROPOSAL (harvest-engine ProposalGenerator via AgentPort)

NOTIFY HUMAN (Telegram via rusvel-channel OR ApprovalCard in UI)

HUMAN APPROVES (ADR-008 approval gate)

SUBMIT
  → Freelancer.com: API POST /api/projects/0.1/bids/ (automated)
  → Upwork: HUMAN submits manually (TOS prohibits automation)
```

### Existing Tools to Study
- **Vollna** — Chrome extension, 30+ filters, AI cover letters, Slack/Telegram alerts
- **Upwex** — AI job scoring, proposal generator, CRM sync
- **n8n template #7782** — Multi-platform RSS + AI proposals + Google Sheets log (open-source workflow)

### What to Build
1. Replace `UpworkRssSource` with `UpworkApiSource` (GraphQL + OAuth)
2. Add `MarketplaceBidPort` trait in rusvel-core
3. Add `FreelancerBidAdapter` implementing MarketplaceBidPort
4. Add `LinkedInRssSource` wrapping RSS.app feed URL from config

---

## 2. Content Machine

### Platform APIs for Posting

| Platform | Auth | Endpoint | Status in RUSVEL |
|---|---|---|---|
| **LinkedIn** | OAuth 2.0 (`w_member_social` scope, open) | `POST /rest/posts` (new REST API) | **Adapter broken** — uses deprecated `/v2/ugcPosts`, needs migration |
| **Twitter/X** | OAuth 2.0 PKCE (`tweet.write` scope) | `POST /2/tweets` | **Auth bug** — bearer token can't post, needs user OAuth token |
| **DEV.to** | API key header | `POST /api/articles` | **Working** — adapter is correct and complete |
| **Hashnode** | Bearer token | GraphQL `mutation PublishPost` | **Not implemented** — easy to add |
| **Medium** | Dead (closed Jan 2025) | N/A | Skip |
| **Substack** | No official API | N/A | Skip — manual only |

### LinkedIn Adapter Migration (Critical)

Current: `POST /v2/ugcPosts` (deprecated)
New: `POST https://api.linkedin.com/rest/posts`
Changes needed:
- New body structure: `commentary` instead of nested `shareCommentary.text`
- Author URN: `urn:li:person:{id}` not `urn:li:organization:0`
- Required header: `Linkedin-Version: {YYYYMM}`
- Post ID from `x-restli-id` response header

### Twitter Auth Fix

Current `TwitterAdapter` uses bearer token → gets 403 on POST.
Fix: Must use OAuth 2.0 PKCE user access token with `tweet.write` scope.
Free tier: 17 tweets/24h (sufficient for 1/day posting).
Thread posting: sequential calls with `reply.in_reply_to_tweet_id` chaining (not currently modeled).

### The Content Machine Workflow

```
CAPTURE (10 min after finishing technical work)
  → Raw notes dump: what you learned, what surprised you, the tradeoff

ANCHOR PIECE (1x/week, 45-90 min)
  → Long-form: DEV.to article or LinkedIn deep post (1,500-3,000 words)
  → This is where human voice matters most

REPURPOSE (AI handles this)
  → 1 LinkedIn post (hook + 3 bullets + CTA, <1,300 chars)
  → 1 tweet thread (6-8 tweets)
  → 2-3 standalone tweets
  → 1 cross-post to Hashnode/DEV.to with canonical_url

SCHEDULE
  → LinkedIn: Tue/Wed/Thu 8-9 AM (3-4x/week)
  → Twitter: 1-3 tweets/day, thread on Tuesday
  → DEV.to: same day as anchor piece

PUBLISH (human approval gate per ADR-008)
```

### What AI Handles vs What Needs Human Touch

**Automatable:** Platform adaptation, character limits, hashtags, scheduling, metrics polling, code-to-content from analysis summaries

**Needs human:** The original insight/capture, hook line (first sentence = 80% of reach), CTA, responding to comments, voice authenticity

### Key Insight: Brand Voice Rules

RUSVEL's content department should have persistent voice rules via `load_rules_for_engine()`:
- Writing style (direct/narrative/technical)
- Audience (AI developers, fullstack builders)
- Emoji policy, CTA patterns, signature
- 5-10 examples of best past posts for style matching

### Tools to Study
- **Typefully** ($8/mo) — writer-first interface, thread editor, minimal UX
- **Taplio** ($39/mo) — LinkedIn-focused, viral post library for inspiration
- **Shield** ($25/mo) — LinkedIn analytics (API access is gated)
- **Buffer** (free tier) — simple multi-platform scheduling

### What to Build
1. Migrate LinkedIn adapter to REST Posts API
2. Fix Twitter auth (OAuth 2.0 PKCE flow)
3. Add Hashnode adapter (GraphQL, simple)
4. Add thread posting for Twitter (sequential with reply chaining)
5. Add `canonical_url` to DEV.to adapter
6. Create voice profile rules for content department
7. Build "inspiration library" — harvest trending posts in niche, surface as drafting seeds

---

## 3. MCP Ecosystem

### Available MCP Servers (Ready to Use)

**Stdio servers (work with RUSVEL today):**

| Server | Command | Auth | Value for RUSVEL |
|---|---|---|---|
| GitHub | `github/github-mcp-server` (Go binary) | `GITHUB_TOKEN` | Issues, PRs, code search — code-engine, harvest-engine |
| Notion | `npx @notionhq/notion-mcp-server` | Bearer token | Knowledge base — forge-engine |
| Slack | `npx @modelcontextprotocol/server-slack` | `SLACK_BOT_TOKEN` | Notifications — channel |
| QuickBooks | `intuit/quickbooks-online-mcp-server` | OAuth | Invoicing — finance-engine |
| Airtable | `npx @airtable/mcp-server` | API key | Lightweight CRM — gtm-engine |

**Remote servers (require HTTP transport — RUSVEL doesn't support yet):**

| Server | Endpoint | Auth | Value |
|---|---|---|---|
| Stripe | Remote OAuth | OAuth 2.1 | Invoicing, payments — finance-engine |
| Calendly | `mcp.calendly.com` | OAuth + DCR | Client scheduling — gtm-engine |
| Atlassian | `mcp.atlassian.com/v1/sse` | OAuth 2.1 | Jira/Confluence — project tracking |
| Google Workspace | Google Cloud hosted | Google OAuth | Gmail, Calendar, Drive, Sheets — everything |
| Pipedream | `mcp.pipedream.com` | Built-in auth | 2,500+ APIs, 10K+ tools |
| Composio | `mcp.composio.dev` | Built-in auth | 500+ integrations |

### Highest-Leverage Code Change: HTTP Transport

Adding Streamable HTTP transport to `rusvel-mcp-client` unlocks ALL remote servers above. Current blocker: `connect_all()` hard-skips `server_type != "stdio"`.

Implementation needs:
- `HttpTransport` alongside existing `StdioTransport`
- URL + bearer token config
- SSE streaming for server-push events
- Rust crates: `reqwest` + `eventsource-stream` or manual SSE parsing + `oauth2`

### Protocol Version Gap

RUSVEL sends `protocolVersion: "2024-11-05"` — one version behind current `2025-03-26`. Works (servers negotiate down) but should update.

### MCP Aggregators (Game Changers)

**Pipedream MCP** — 2,500+ APIs, 10K+ tools, hosted, built-in auth management
**Composio MCP** — 500+ integrations, AI-native, tool selection to avoid context pollution

Either one gives RUSVEL access to virtually every SaaS tool through a single MCP connection.

### Safe Browser Automation Alternatives

| Platform | Safe Alternative |
|---|---|
| Upwork | Email notifications parsed via Gmail MCP |
| LinkedIn | RSS.app converted search feed |
| Twitter/X | Official API (Basic $100/mo) |
| Generic | Bright Data / Apify managed scrapers |

---

## 4. Competing Products

### AI Agent Platforms

| Product | What It Does | Key Insight | Price | Open-Source |
|---|---|---|---|---|
| **Dust.tt** | Team agent workspace with data source connections | "Data connections as first-class citizens" > raw agent power | EUR 29/user/mo | No |
| **Relevance AI** | No-code multi-agent builder, "workforce" model | "Hire a role" framing > "configure an agent" | $19/mo (10K credits) | No |
| **CrewAI** | Python multi-agent framework | Crew-of-specialists model validates RUSVEL's multi-dept design | Free (OSS) / $25/mo hosted | Framework: yes |
| **Julep** | Stateful AI workflow platform (YAML-first) | **Shut down Dec 2025** — validates architecture, business model failed | Self-host only now | Yes |
| **Dify** | LLMOps platform (100K+ GitHub stars) | Best OSS observability: logs, performance, prompt improvement | Free self-host / $59/mo cloud | Yes (Apache 2.0) |
| **Langflow/Flowise** | Visual canvas LLM builders | Canvas metaphor powerful for builders, overwhelming for end-users | Free self-host | Yes |

### Automation Platforms

| Product | Key Insight for RUSVEL |
|---|---|
| **n8n** | Mix deterministic + AI steps in one workflow (= RUSVEL's flow-engine). 600 templates = distribution strategy. Step-by-step visual replay is critical UX. |
| **Activepieces** | MCP-first positioning. Unlimited execution on paid plans removes anxiety. MIT license. |
| **Make.com** | **Reasoning Panel** = show agent's decision chain in real-time. Most important UX pattern in category. Agents + automation on same canvas. |
| **Zapier** | 7K integrations = unbeatable breadth. MCP is RUSVEL's answer. "Describe what you want" Copilot UX = `!build` command. |

### Solo Builder / Personal AI

| Product | Key Insight for RUSVEL |
|---|---|
| **Granola** | **"Augment, don't replace"** — AI enhances what human wrote, invisible UX, 70% retention. |
| **Notion AI** | "AI where your work already is" — not a separate agent UI, AI embedded in every surface. |
| **Mem.ai** | Folderless capture-then-organize. RUSVEL's memory + vector store = same backend, needs UX surface. |

### Content & Outreach Tools

| Product | Key Insight for RUSVEL |
|---|---|
| **Taplio** ($39/mo) | Viral post library (inspiration-first) is stickiest feature. RUSVEL could harvest trending posts as drafting seeds. |
| **Typefully** ($8/mo) | Writer-first, platform-second. Clean writing surface > form with 20 fields. |
| **Copy.ai** ($29-249/mo) | "Infobase" (brand knowledge store feeding all workflows) = stickiness moat. = RUSVEL memory + rules. |
| **Clay** ($185/mo) | Data quality > copy quality for outreach. Waterfall enrichment. Expensive = RUSVEL opportunity. |
| **Apollo.io** ($59/mo) | Database + sequencer + dialer in one. RUSVEL can't match 210M contacts but can do 80% for a niche. |

### Solo Builder Pain Points (from surveys)

| Pain Point | RUSVEL's Answer |
|---|---|
| Too many subscriptions, each solving one thing | Core thesis — one binary |
| Tools don't talk to each other | flow-engine + MCP client |
| Unpredictable usage-based pricing | Local-first = fixed cost |
| AI content feels generic | Brand voice rules + memory |
| Switching between tools breaks flow | Single UI |
| Can't see what AI decided or why | AgentEvent reasoning panel |
| Free tiers useless after onboarding | No per-action billing |
| Agent loops spiral out of control | ADR-008 approval gates |

---

## 5. RUSVEL's Real Differentiators

| Advantage | Why No Competitor Matches It |
|---|---|
| Single binary, `cargo install` | All others need Docker, cloud, or Python/Node |
| Local-first, zero cloud dependency | Even self-hosted n8n/Dify need a running server |
| Unlimited executions, your hardware | Every cloud tool has per-action costs |
| 14 domain departments in one system | No single tool covers all business functions |
| Human approval gates as architecture (ADR-008) | Most tools bolt on approval as an afterthought |
| MCP client + server in one binary | No self-hosted product does both |
| Swap any LLM provider locally | Cloud tools lock you to their model choices |

---

## 6. Priority UX Patterns to Implement

Ranked by competitive evidence of user value:

1. **Reasoning Panel** — Show agent's decision chain during tool-use loops (Make.com proves this is #1 trust builder). RUSVEL has `AgentEvent` streaming; needs prominent frontend panel.

2. **Brand Voice / Infobase** — "Set voice once, use everywhere" (Copy.ai's stickiness moat). Add to onboarding + persist in memory as privileged context for all content generation.

3. **Template Library** — Shareable flow/agent templates (n8n uses 600+ templates as primary acquisition channel). Even 10-20 curated solo-founder templates would dramatically reduce time-to-value.

4. **Same-Canvas: Automation + Agent** — Agents and workflows in one view (Make.com insight). RUSVEL's `/flows` and `/chat` are separate; unify or deeply link.

5. **Knowledge Inbox** — Quick-capture surface connected to memory (Mem.ai, Granola pattern). "Capture then organize" > "organize then use."

---

## 7. Action Items (Prioritized)

### Must Fix (broken things)
1. Replace dead `UpworkRssSource` with GraphQL API or email monitoring
2. Migrate LinkedIn adapter from `/v2/ugcPosts` to `/rest/posts`
3. Fix Twitter adapter auth (bearer → OAuth 2.0 PKCE user token)

### High-Impact New Work
4. Add Streamable HTTP transport to `rusvel-mcp-client` (unlocks Google, Stripe, Calendly, Pipedream, Composio)
5. Add `FreelancerBidAdapter` with API bid submission
6. Add brand voice onboarding + content rules
7. Build reasoning panel in frontend (surface AgentEvent prominently)

### Medium-Impact
8. Add Hashnode adapter
9. Add Twitter thread posting (sequential with reply chaining)
10. Add `canonical_url` to DEV.to adapter
11. Create 10-20 flow templates for common solo-builder tasks
12. Build inspiration library (harvest trending posts → content seeds)

### Architecture
13. Add `MarketplaceBidPort` trait to rusvel-core
14. Update MCP protocol version to `2025-03-26`

---

## Sources

### Freelancer Platforms
- [Upwork Developer Portal](https://www.upwork.com/developer)
- [Upwork GraphQL Docs](https://www.upwork.com/developer/documentation/graphql/api/docs/index.html)
- [Upwork TOS on Automation](https://support.upwork.com/hc/en-us/articles/43342677368467)
- [Freelancer Developer Portal](https://developers.freelancer.com/)
- [n8n Multi-Platform Workflow #7782](https://n8n.io/workflows/7782)
- [Vollna](https://www.vollna.com/)

### Content Platforms
- [LinkedIn REST Posts API](https://learn.microsoft.com/en-us/linkedin/marketing/community-management/shares/posts-api)
- [LinkedIn Migration Guide](https://learn.microsoft.com/en-us/linkedin/marketing/community-management/contentapi-migration-guide)
- [X API Rate Limits](https://docs.x.com/x-api/fundamentals/rate-limits)
- [DEV.to API](https://developers.forem.com/api/v0)
- [Hashnode GraphQL API](https://docs.hashnode.com/quickstart/introduction)

### MCP Ecosystem
- [Official MCP Registry](https://registry.modelcontextprotocol.io/)
- [github/github-mcp-server](https://github.com/github/github-mcp-server)
- [Stripe MCP](https://docs.stripe.com/mcp)
- [Notion MCP](https://developers.notion.com/docs/mcp)
- [Pipedream MCP](https://pipedream.com/docs/connect/mcp)
- [Composio MCP](https://composio.dev/)
- [MCP Roadmap](https://modelcontextprotocol.io/development/roadmap)

### Competing Products
- [Dust.tt](https://dust.tt)
- [Dify](https://dify.ai/) (Apache 2.0, 100K+ stars)
- [n8n](https://n8n.io/)
- [Make.com AI Agents](https://www.make.com/en/blog/announcing-next-generation-make-ai-agents)
- [Granola UX Analysis](https://uxplanet.org/the-art-of-invisible-ai-what-granolas-70-retention-teaches-us-about-product-design)
- [Copy.ai GTM Platform](https://www.copy.ai/)
- [Clay](https://www.clay.com/)
- [Stack Overflow 2025 Developer Survey](https://survey.stackoverflow.co/2025/)
