# RUSVEL — Three Core Workflows Design

> Designed 2026-04-01. Product-first, not architecture-first.
> Each workflow answers: what does Mehdi do, what does the app do, what does the screen look like?

---

## Workflow 1: Find Work

**Goal:** Never miss a good gig. Get notified fast. Send great proposals with minimal effort.

### The Daily Experience

**Morning (automatic, you're not even looking):**
1. RUSVEL polls sources every 15 minutes:
   - Freelancer.com API (search by your skills: Rust, AI, fullstack, agent)
   - Upwork GraphQL API (same keywords)
   - PPH/Guru RSS feeds
   - LinkedIn saved search via RSS.app
2. Each new listing is deduplicated, scored against your profile:
   - Skills match (Rust, AI agents, fullstack, SvelteKit)
   - Budget range (your minimum hourly/fixed)
   - Client quality signals (reviews, spend history, verified payment)
   - Competition level (number of bids)
3. Score > threshold → RUSVEL drafts a proposal automatically using your profile + past winning proposals as context

**You open the app (or get a Telegram/email notification):**

```
┌─────────────────────────────────────────────────────┐
│  🎯 3 New Opportunities (scored > 7/10)             │
│                                                      │
│  ┌─────────────────────────────────────────────┐    │
│  │ [9.2] "Build AI Agent System in Rust"       │    │
│  │  Freelancer.com · $5K-10K · 3 bids · 2h ago│    │
│  │  Skills: Rust, AI, API, WebSockets          │    │
│  │                                              │    │
│  │  Draft proposal ready · [Review] [Skip]     │    │
│  └─────────────────────────────────────────────┘    │
│                                                      │
│  ┌─────────────────────────────────────────────┐    │
│  │ [8.1] "Full-Stack Dev for SaaS Dashboard"   │    │
│  │  Upwork · $50-80/hr · 12 bids · 4h ago     │    │
│  │  Skills: SvelteKit, TypeScript, REST API    │    │
│  │                                              │    │
│  │  Draft proposal ready · [Review] [Skip]     │    │
│  └─────────────────────────────────────────────┘    │
│                                                      │
│  Pipeline: 3 new → 2 proposed → 1 interviewing     │
└─────────────────────────────────────────────────────┘
```

**You click [Review]:**

```
┌──────────────────────────────────────────────────────┐
│  Proposal for "Build AI Agent System in Rust"        │
│  Score: 9.2/10 · Why: exact skills match, high       │
│  budget, low competition, verified client             │
│                                                       │
│  ┌────────────────────────────────────────────┐      │
│  │ Hi [Client Name],                          │      │
│  │                                            │      │
│  │ I'm a fullstack developer specializing in  │      │
│  │ Rust-based AI agent systems. I recently    │      │
│  │ built RUSVEL — a 55-crate Rust workspace   │      │
│  │ with streaming agent runtime, tool         │      │
│  │ orchestration, and multi-LLM support...    │      │
│  │                                            │      │
│  │ [editable text area — you tweak as needed] │      │
│  └────────────────────────────────────────────┘      │
│                                                       │
│  Bid: $7,500 (suggested from budget range)           │
│  Timeline: 4 weeks (suggested)                        │
│                                                       │
│  [Submit to Freelancer.com]  [Edit More]  [Reject]   │
│                                                       │
│  ⚡ Freelancer.com: auto-submits via API              │
│  ⚠️  Upwork: copies to clipboard, opens listing       │
└──────────────────────────────────────────────────────┘
```

**After submission:**
- Pipeline view tracks: Proposed → Interviewing → Won/Lost
- Won/lost outcomes feed back into scoring (RAG via vector store)
- Over time, proposals get better because the AI learns what wins

### What Exists vs What's Needed

| Step | Exists? | What's Needed |
|---|---|---|
| Source polling | Partial — RSS sources exist, Upwork RSS dead | Fix Upwork (GraphQL), add LinkedIn RSS.app source |
| Deduplication | Yes — content_hash + platform_job_key | Working |
| Scoring | Yes — LLM + keyword dual method | Add profile-based weighting from UserProfile |
| Proposal generation | Yes — ProposalGenerator via AgentPort | Add past-winning-proposals as RAG context |
| Notification | Partial — Telegram channel exists | Wire scoring → notification trigger |
| Pipeline tracking | Yes — full pipeline stages in harvest-engine | Working |
| Auto-submit (Freelancer) | No | Build FreelancerBidAdapter |
| Clipboard + open URL (Upwork) | No | Frontend helper — small |
| Outcome feedback to scoring | Partial — configure_rag exists | Enable embeddings at boot |
| Approval UI | Yes — ApprovalCard exists | Wire to proposal review flow |

---

## Workflow 2: Build Brand (Content Machine)

**Goal:** Post 3-4x/week on LinkedIn + Twitter + DEV.to with your authentic voice. Spend < 30 min/day total.

### The Weekly Rhythm

**Anytime (capture — 2 minutes):**
You just solved a hard problem, shipped a feature, or learned something. Quick capture:

```
┌──────────────────────────────────────────────────────┐
│  Quick Capture                                [Cmd+K]│
│                                                       │
│  ┌────────────────────────────────────────────┐      │
│  │ Built a streaming agent runtime in Rust    │      │
│  │ with deferred tool loading — saves 85%     │      │
│  │ tokens. The trick was making tool_search   │      │
│  │ a meta-tool that dynamically injects       │      │
│  │ discovered tools into the next request...  │      │
│  └────────────────────────────────────────────┘      │
│                                                       │
│  Tags: [rust] [ai-agents] [architecture]             │
│  [Save to Knowledge]                                  │
└──────────────────────────────────────────────────────┘
```

Saved to `rusvel-memory` with FTS5 indexing. This is your raw material.

**Sunday evening (anchor piece — 30-45 min with AI assist):**

```
┌──────────────────────────────────────────────────────┐
│  Content Studio                                       │
│                                                       │
│  This week's captures: 4 notes                        │
│  ┌────────────────────────────────────────────┐      │
│  │ ● Streaming agent runtime (Tue)            │      │
│  │ ● Deferred tool loading trick (Tue)        │      │
│  │ ● SQLite WAL for concurrent agents (Thu)   │      │
│  │ ● Why hexagonal architecture matters (Fri) │      │
│  └────────────────────────────────────────────┘      │
│                                                       │
│  [Generate Anchor Piece]  [Pick captures to use]     │
│                                                       │
│  AI drafts long-form using YOUR voice rules:          │
│  "Direct, technical, first-person. Short sentences.   │
│   Code examples when relevant. No fluff."             │
│                                                       │
│  ┌────────────────────────────────────────────┐      │
│  │ # How I Built a Streaming Agent Runtime    │      │
│  │ # in Rust (And Why It Saves 85% on Tokens) │      │
│  │                                            │      │
│  │ Last week I needed agents that could call  │      │
│  │ 22 tools without blowing my token budget...│      │
│  │                                            │      │
│  │ [full markdown editor — you refine]        │      │
│  └────────────────────────────────────────────┘      │
│                                                       │
│  [Repurpose →]  Ready to generate platform versions   │
└──────────────────────────────────────────────────────┘
```

**Click [Repurpose →] — AI generates platform variants:**

```
┌──────────────────────────────────────────────────────┐
│  Platform Variants                                    │
│                                                       │
│  LinkedIn Post (1,247 chars)                   [Edit] │
│  ┌────────────────────────────────────────────┐      │
│  │ Most AI agent frameworks waste 85% of      │      │
│  │ tokens loading tools the agent never uses.  │      │
│  │                                            │      │
│  │ Here's how I fixed it in Rust:             │      │
│  │ • tool_search as a meta-tool...            │      │
│  │ • Dynamic injection into next request...   │      │
│  │                                            │      │
│  │ Full writeup on DEV.to (link in comments)  │      │
│  └────────────────────────────────────────────┘      │
│                                                       │
│  Tweet Thread (6 tweets)                       [Edit] │
│  DEV.to Article (full, with canonical_url)     [Edit] │
│  Hashnode Cross-post                           [Edit] │
│                                                       │
│  Schedule:                                            │
│  ● LinkedIn → Tue 8:30 AM                            │
│  ● Thread   → Tue 9:00 AM                            │
│  ● DEV.to   → Tue (publish now)                      │
│  ● Hashnode  → Wed (cross-post)                      │
│                                                       │
│  [Schedule All]  [Approve & Publish Now]              │
└──────────────────────────────────────────────────────┘
```

**During the week (automated with approval):**
- Scheduled posts publish automatically (after initial approval)
- 2-3 standalone tweets extracted from the anchor piece publish on other days
- Analytics update in the background: impressions, comments, profile visits
- Weekly summary: "Your LinkedIn post got 3,200 impressions, 47 comments. Best performing topic: Rust + AI."

### Code-to-Content Shortcut

You just shipped a feature. Instead of capturing manually:

```
rusvel content from-code ./crates/rusvel-agent
```

→ Analyzes code changes, generates anchor piece about the architecture/decisions/learnings.
→ This already exists in content-engine! Just needs the voice rules and platform fixes.

### What Exists vs What's Needed

| Step | Exists? | What's Needed |
|---|---|---|
| Quick capture to memory | Partial — memory API exists | Frontend capture UI (Cmd+K or sidebar) |
| Voice rules | No | Create content dept rules with writing style samples |
| Anchor piece generation | Yes — ContentWriter::draft | Wire memory captures as context input |
| Platform adaptation | Yes — writer.rs has platform prompts | Working |
| LinkedIn posting | Broken — deprecated API | Migrate adapter to REST Posts API |
| Twitter posting | Broken — wrong auth | Fix OAuth 2.0 PKCE, add thread posting |
| DEV.to posting | Working | Add canonical_url field |
| Hashnode posting | No | Build adapter (simple GraphQL) |
| Scheduling | Yes — calendar.rs + job queue | Working |
| Approval gate | Yes — ADR-008 | Working |
| Analytics | Partial — analytics.rs exists | DEV.to metrics work; LinkedIn gated; Twitter limited |
| Code-to-content | Yes — from-code endpoint | Wire voice rules into prompts |

---

## Workflow 3: Win Clients (Outreach → Invoice)

**Goal:** Once you find a lead (from Workflow 1 or elsewhere), manage the relationship from first contact to getting paid.

### The Flow

**A lead comes in** (from harvest pipeline, or you add manually):

```
┌──────────────────────────────────────────────────────┐
│  Contacts                                     [+ Add]│
│                                                       │
│  ┌─────────────────────────────────────────────┐    │
│  │ Sarah Chen · CTO @ DataFlow Inc             │    │
│  │ Source: Upwork proposal (won)               │    │
│  │ Stage: Negotiation · Last contact: 2d ago   │    │
│  │ Deal: $8,000 fixed · "AI Pipeline Builder"  │    │
│  │                                              │    │
│  │ [Open Deal] [Send Follow-up] [Add Note]     │    │
│  └─────────────────────────────────────────────┘    │
│                                                       │
│  ┌─────────────────────────────────────────────┐    │
│  │ James Liu · Founder @ CodeShip              │    │
│  │ Source: LinkedIn DM                         │    │
│  │ Stage: Lead · No response yet               │    │
│  │ Next action: Follow-up email (scheduled)    │    │
│  │                                              │    │
│  │ [View Sequence] [Skip Follow-up]            │    │
│  └─────────────────────────────────────────────┘    │
│                                                       │
│  Pipeline: 5 leads → 2 negotiation → 1 closed       │
└──────────────────────────────────────────────────────┘
```

**Outreach sequences (AI-drafted, human-approved):**

```
┌──────────────────────────────────────────────────────┐
│  Outreach Sequence: James Liu                         │
│                                                       │
│  Step 1: Initial email ✅ Sent 3 days ago             │
│  Step 2: Follow-up    ⏰ Scheduled tomorrow 9 AM     │
│  Step 3: Final check  📋 If no response, close        │
│                                                       │
│  ┌────────────────────────────────────────────┐      │
│  │ Hi James,                                  │      │
│  │                                            │      │
│  │ Following up on my note about the Rust     │      │
│  │ agent system. I noticed CodeShip is        │      │
│  │ scaling their ML pipeline — I've built     │      │
│  │ exactly this kind of infrastructure...     │      │
│  │                                            │      │
│  │ [editable — your voice, AI-assisted]       │      │
│  └────────────────────────────────────────────┘      │
│                                                       │
│  [Approve & Send] [Edit] [Delay 2 Days]              │
│                                                       │
│  Sends via: Gmail (MCP) · approved by you first       │
└──────────────────────────────────────────────────────┘
```

**Deal closes → Invoice:**

```
┌──────────────────────────────────────────────────────┐
│  Invoice #INV-2026-042                                │
│  Client: Sarah Chen · DataFlow Inc                    │
│  Project: AI Pipeline Builder                         │
│                                                       │
│  ┌────────────────────────────────────────────┐      │
│  │ AI Agent System Design    20 hrs × $100    │      │
│  │ Implementation            40 hrs × $100    │      │
│  │ Testing & Deployment      10 hrs × $100    │      │
│  │                                            │      │
│  │ Subtotal:               $7,000             │      │
│  │ Tax (if applicable):    $0                 │      │
│  │ Total:                  $7,000             │      │
│  └────────────────────────────────────────────┘      │
│                                                       │
│  [Send via Email] [Download PDF] [Mark Paid]          │
│                                                       │
│  Sends via: Gmail (MCP)                               │
│  Payment tracking in finance department               │
└──────────────────────────────────────────────────────┘
```

### What Exists vs What's Needed

| Step | Exists? | What's Needed |
|---|---|---|
| Contact management | Yes — GTM CRM (deals, contacts, stages) | Frontend pages exist (contacts, deals) |
| Outreach sequences | Yes — gtm-engine outreach with follow-ups | Working |
| Email sending | Stub — mock adapter | Wire Gmail MCP (or SMTP via env vars) |
| AI-drafted emails | Partial — AgentPort can generate | Add outreach templates + voice rules |
| Approval gate on send | Yes — ADR-008 OutreachSend | Working |
| Deal pipeline tracking | Yes — deal stages in GTM | Frontend exists |
| Invoicing | Yes — gtm-engine invoicing with tax | Frontend exists (367 lines) |
| Invoice PDF generation | No | Need PDF rendering (or HTML email) |
| Payment tracking | Partial — can mark paid | Wire to finance-engine ledger |
| Gmail integration | No | Add Gmail MCP (stdio or HTTP transport) |

---

## Which Workflow First?

**Recommendation: Workflow 2 (Content Machine)**

Why:
1. **Closest to working** — content-engine has real logic, adapters exist (just need fixes), code-to-content works
2. **Daily value immediately** — you post to LinkedIn/Twitter/DEV.to this week, not someday
3. **Compounds** — content builds your brand, brand attracts clients, clients = revenue
4. **Smallest gap to close** — fix LinkedIn adapter, fix Twitter auth, add voice rules, add capture UI. That's it.
5. **Feeds Workflow 1** — your content attracts inbound leads, reducing dependence on outbound gig hunting

**Sequence:**
1. Content Machine (1-2 weeks to make real) → start posting immediately
2. Find Work (2-3 weeks) → freelancer API integrations, scoring tuned to your profile
3. Win Clients (ongoing) → CRM + email once Gmail MCP is wired

---

## The First Sprint (Content Machine MVP)

### Day 1-2: Fix what's broken
- [ ] Migrate LinkedIn adapter to REST Posts API
- [ ] Fix Twitter OAuth 2.0 PKCE (or skip Twitter initially, start LinkedIn-only)
- [ ] Add canonical_url to DEV.to adapter

### Day 3-4: Add voice
- [ ] Create content department rules with your writing style + samples
- [ ] Wire rules into ContentWriter::draft prompts
- [ ] Add UserProfile context to content generation

### Day 5-6: Build the capture + repurpose flow
- [ ] Quick capture UI (Cmd+K or sidebar widget) → saves to memory
- [ ] "Generate from captures" button → anchor piece draft
- [ ] "Repurpose" button → platform variants with scheduling

### Day 7: First real post
- [ ] Write/refine an anchor piece about building RUSVEL
- [ ] Let the app generate LinkedIn + DEV.to versions
- [ ] Approve and publish
- [ ] Iterate from there

### What we deliberately skip for now:
- Hashnode adapter (nice to have, not critical)
- Twitter threads (start with single tweets or skip Twitter)
- Analytics beyond what DEV.to provides
- Inspiration library (future iteration)
- Template library (future iteration)
