# Freelancer-OS Research Questions — Raw Notes (2026-04-01)

> Historical session transcript, filed as-is from repo root (`plan4.md`).
> Proposes a research pass (Upwork/Toptal APIs, LinkedIn content pipelines,
> MCP server landscape, competitor teardown of Dust.tt/Relevance AI/n8n)
> before designing the three core workflows: find work, build brand, win
> clients. As of 2026-09-02, superseded in direction by the ledger decision
> `strategy-continue-rust-not-bun-rewrite` — any research here should be
> re-scoped against the existing Rust harvest/content/gtm engines rather than
> a fresh stack.

---


  And here's the critical insight: you use Claude Code all day. Your app doesn't need to replace that. It needs to be the business brain running alongside it — the thing
  that finds you work, builds your brand, and manages your pipeline while you build.

  What I'd Propose

  Before we design anything, I think we need to research first. You said it yourself — maybe you need to study and learn more. I agree. Specifically:

  1. How do the best freelancer automation tools work? What APIs does Upwork/Toptal/Freelancer.com expose? What can CDP scrape? What's the real flow from "gig posted" to
  "proposal sent"?
  2. What does a real content machine look like? How do solo builders who post daily on LinkedIn/Twitter/DEV.to actually do it? What's the workflow? What can be automated
  vs. what needs human voice?
  3. What MCP servers and APIs exist today that could give your app superpowers without building from scratch? (Gmail, Calendar, LinkedIn, GitHub — some of these you already
   have connected via MCP)
  4. What are competing products doing? Dust.tt, Relevance AI, n8n, Activepieces — what do they get right?

  Want me to do a deep research session on these 4 questions? I can run parallel research agents and come back with concrete findings — not abstract architecture, but
  "here's how Upwork's API works, here's what a LinkedIn content pipeline looks like, here's what MCP servers are available today."

  That research would give us the ground truth to then design the 3 core workflows:
  - Find work (harvest → score → propose)
  - Build brand (draft → adapt → publish → track)
  - Win clients (outreach → follow-up → close → invoice)
