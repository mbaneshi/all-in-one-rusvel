# Web Presence Engine — The Solo Builder's Eyes, Hands, and Network

> **RUSVEL needs to BE on the internet, not just talk about it.**
>
> Every platform a solo builder uses — Upwork, Freelancer, LinkedIn, GitHub,
> Toptal, Twitter, ProductHunt — is a surface where RUSVEL should passively
> observe, actively participate, and autonomously operate. Across multiple
> accounts, multiple machines, with human behavior, security, and a cloud
> dashboard that makes it all visible.
>
> Date: 2026-03-30 | Status: Proposal

---

## Table of Contents

1. [The Problem](#1-the-problem)
2. [Vision: What the Solo Builder Needs](#2-vision-what-the-solo-builder-needs)
3. [Current State](#3-current-state)
4. [Architecture: Web Presence as a Port](#4-architecture-web-presence-as-a-port)
5. [The Platform Abstraction](#5-the-platform-abstraction)
6. [Browser Fleet: Multi-Machine, Multi-Account](#6-browser-fleet-multi-machine-multi-account)
7. [Passive Observation Layer](#7-passive-observation-layer)
8. [Active Interaction Layer](#8-active-interaction-layer)
9. [Human Behavior & Security](#9-human-behavior--security)
10. [Cloud Coordination (Mesh)](#10-cloud-coordination-mesh)
11. [Integration with Every Department](#11-integration-with-every-department)
12. [Auto-Capability: Agents Equip Themselves](#12-auto-capability-agents-equip-themselves)
13. [Domain Model](#13-domain-model)
14. [Port & Trait Design](#14-port--trait-design)
15. [Implementation Phases](#15-implementation-phases)
16. [Open Questions for Research](#16-open-questions-for-research)

---

## 1. The Problem

RUSVEL has a powerful runtime: 55 crates, 14 departments, agent loop, tool registry, flow engine, god agent. But it's **blind and armless on the internet**. It can't:

- See what's on Upwork right now (only mock data or basic RSS)
- Browse LinkedIn as you, observe who viewed your profile
- Submit a proposal on Freelancer
- Post content to Twitter/LinkedIn/DEV.to with your real account
- Monitor GitHub issues on client repos
- Check email for client responses
- Act on any platform as your authenticated self

The harvestor solved this for Upwork/Freelancer with Python/TS. But that violates the single-binary rule and only covers 2 platforms. RUSVEL needs this capability **natively, for every platform, across multiple machines**.

This isn't a feature. It's the foundation that makes the entire "virtual agency" concept real.

---

## 2. Vision: What the Solo Builder Needs

### Day in the Life (2027)

```
06:00  RUSVEL wakes up. 12 Chrome profiles across 3 machines.
       Each profile is logged into a different platform/account.

06:01  Harvest dept scans all platforms passively.
       Upwork (2 accounts): 47 new jobs matching your skills.
       Freelancer: 12 new projects.
       LinkedIn: 3 job posts from connections.
       GitHub: 2 repos looking for Rust contributors.

06:05  Scorer runs. 8 opportunities score > 80.
       God agent reviews: "3 of these are similar to jobs you won last month."

06:10  Capability engine auto-equips for top 3 jobs:
       - Job #1 (dental chatbot): installs healthcare-api MCP, creates HIPAA rule
       - Job #2 (Rust CLI tool): already equipped, skips
       - Job #3 (SvelteKit dashboard): installs chart-library skill

06:15  Code dept scaffolds demo for Job #1.
       Infra dept deploys to demo-dental.rusvel.dev.
       Content dept creates case study screenshot.

06:20  Harvest dept generates proposals for all 3.
       Each references your live demo URL and relevant portfolio.
       → Queued for your approval.

06:30  You wake up. Open RUSVEL dashboard on your phone.
       See 3 proposals. Edit one. Approve all.
       RUSVEL submits via browser_act on each platform.

08:00  Client on Freelancer responds to yesterday's proposal.
       RUSVEL detects the message (passive observation).
       GTM dept notifies you via Telegram.
       Suggests a response draft.

10:00  You win Job #2. RUSVEL detects "Hired" status change.
       Legal dept generates contract template.
       Finance dept creates invoice schedule.
       Code dept clones client's repo, starts analysis.
       Forge dept creates daily standup schedule.

12:00  Content dept drafts a "New project announcement" for LinkedIn.
       → Queued for approval. You approve.
       → RUSVEL posts via your LinkedIn Chrome profile.
```

**This is not science fiction. Every piece exists in isolation. The proposal is about wiring them together.**

---

## 3. Current State

### What EXISTS in RUSVEL

| Component | File | Status |
|-----------|------|--------|
| `BrowserPort` trait | `rusvel-core/src/ports.rs:569-585` | 6 methods: connect, disconnect, tabs, observe, evaluate_js, navigate |
| `CdpClient` | `rusvel-cdp/src/lib.rs` | Single instance, passive observation, broadcast events |
| `CdpPool` | `rusvel-cdp/src/pool.rs` | **Stub** — config struct + endpoint lookup, no connection management |
| `NetworkCapture` | `rusvel-cdp/src/network.rs` | **Stub** — empty struct, no Network.enable wiring |
| `platforms/upwork.rs` | `rusvel-cdp/src/platforms/upwork.rs` | Exists, unknown depth |
| `CdpSource` | `harvest-engine/src/cdp_source.rs` | Works — navigate + evaluate_js + parse, falls back to mock |
| `browser_observe` tool | `rusvel-builtin-tools/src/browser.rs` | Works — subscribes to tab events |
| `browser_search` tool | `rusvel-builtin-tools/src/browser.rs` | **Stub** — returns empty results |
| `browser_act` tool | `rusvel-builtin-tools/src/browser.rs` | Works — navigate + evaluate_js with approval gate |
| `BrowserEvent` | `rusvel-core/src/domain.rs` | DataCaptured, Navigation, TabChanged |
| `BrowsingMode` | `rusvel-core/src/domain.rs` | Passive, Assisted, Autonomous, Vision |
| `HarvestSource` trait | `harvest-engine/src/source.rs` | 4 implementations: Mock, CdpSource, UpworkRss, FreelancerRss |
| Chrome profiles | `~/.chrome_profiles/` | 13 profiles on your machine (baneshi, ayoub, leila, etc.) |

### What's MISSING

| Gap | Impact |
|-----|--------|
| `NetworkCapture` is empty | No passive network interception — the core feature of the harvestor |
| `CdpPool` is a static lookup | No connection lifecycle, no health checks, no multi-instance management |
| No platform extractors | No `__NUXT__` parsing, no ngrx parsing, no GraphQL interception |
| No write actions beyond navigate | Can't fill forms, click buttons, submit proposals |
| Single machine only | No coordination across VPS instances |
| No human behavior simulation | Detectable as bot — fixed timing, no scrolling, no mouse movement |
| No credential isolation | All profiles share one RUSVEL instance, no per-account auth |
| No platform abstraction | Each platform is ad-hoc — no unified "PlatformPort" concept |

---

## 4. Architecture: Web Presence as a Port

The key insight: **every online platform is a port adapter**, just like SQLite, Ollama, or Telegram. The platform exposes capabilities (search jobs, post content, send messages) and RUSVEL accesses them through a trait.

```
┌──────────────────────────────────────────────────────────┐
│                    rusvel-core                             │
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │              PlatformPort (trait)                  │   │
│  │                                                    │   │
│  │  fn capabilities() → Vec<PlatformCapability>      │   │
│  │  fn observe() → Receiver<PlatformEvent>           │   │
│  │  fn execute(action: PlatformAction) → Result      │   │
│  │  fn search(query: PlatformQuery) → Vec<Item>      │   │
│  │  fn status() → ConnectionStatus                   │   │
│  └──────────────────────────────────────────────────┘   │
│                           ▲                              │
│                           │ implements                   │
│                           │                              │
└───────────────────────────┼──────────────────────────────┘
                            │
          ┌─────────────────┼─────────────────┐
          │                 │                 │
  ┌───────┴──────┐  ┌──────┴───────┐  ┌─────┴───────┐
  │UpworkAdapter │  │FreelancerAdp │  │LinkedInAdp  │
  │              │  │              │  │             │
  │ CDP + __NUXT__│  │ CDP + ngrx  │  │ CDP + DOM  │
  │ + GraphQL    │  │ + REST API  │  │ + API      │
  └──────┬───────┘  └──────┬───────┘  └─────┬───────┘
         │                 │                │
         └─────────────────┼────────────────┘
                           │
                    ┌──────┴──────┐
                    │ BrowserPort │  (existing, extended)
                    │             │
                    │ CDP client  │
                    │ per-profile │
                    └─────────────┘
```

**The platform adapter uses BrowserPort (CDP) as its transport, but exposes a higher-level API.** The agent doesn't call `evaluate_js("window.__NUXT__")` — it calls `upwork.search_jobs("ai developer")`.

---

## 5. The Platform Abstraction

### PlatformPort Trait

```rust
// rusvel-core/src/ports.rs — NEW port trait

/// A platform the solo builder operates on (Upwork, LinkedIn, GitHub, etc.).
///
/// Each platform adapter uses BrowserPort (CDP) or direct API access as transport.
/// Agents interact with platforms through this trait, never through raw CDP.
#[async_trait]
pub trait PlatformPort: Send + Sync {
    /// Platform identifier: "upwork", "freelancer", "linkedin", "github"
    fn platform_id(&self) -> &str;

    /// What this platform can do (read jobs, post content, send messages, etc.)
    fn capabilities(&self) -> Vec<PlatformCapability>;

    /// Current connection status (which accounts are live)
    async fn status(&self) -> PlatformStatus;

    /// Passive observation — stream of events from the platform
    /// (new jobs, messages, profile views, status changes)
    async fn observe(&self, account_id: &str)
        -> Result<tokio::sync::broadcast::Receiver<PlatformEvent>>;

    /// Search for items (jobs, people, posts) on the platform
    async fn search(&self, account_id: &str, query: PlatformQuery)
        -> Result<Vec<PlatformItem>>;

    /// Execute an action (submit proposal, send message, post content)
    /// Returns PendingAction if approval is required.
    async fn execute(&self, account_id: &str, action: PlatformAction)
        -> Result<ActionResult>;

    /// Read a specific item's full details
    async fn get_item(&self, account_id: &str, item_id: &str)
        -> Result<PlatformItem>;
}
```

### Platform Domain Types

```rust
// rusvel-core/src/domain.rs — NEW types

/// What a platform can do
pub enum PlatformCapability {
    SearchJobs,
    SearchPeople,
    SearchPosts,
    SubmitProposal,
    SendMessage,
    PostContent,
    ReadNotifications,
    ReadMessages,
    ReadAnalytics,
    ManageProfile,
}

/// Event from passive observation
pub enum PlatformEvent {
    /// New item discovered (job listing, post, notification)
    ItemDiscovered {
        platform: String,
        account_id: String,
        item: PlatformItem,
    },
    /// Message received
    MessageReceived {
        platform: String,
        account_id: String,
        from: String,
        preview: String,
        thread_id: String,
    },
    /// Status change (hired, proposal viewed, etc.)
    StatusChanged {
        platform: String,
        account_id: String,
        item_id: String,
        old_status: String,
        new_status: String,
    },
    /// Network data captured (raw, for platform-specific parsing)
    RawCapture {
        platform: String,
        account_id: String,
        kind: String,
        data: serde_json::Value,
    },
}

/// Action to perform on a platform
pub enum PlatformAction {
    SubmitProposal {
        job_id: String,
        cover_letter: String,
        rate: Option<f64>,
        attachments: Vec<String>,
    },
    SendMessage {
        to: String,
        thread_id: Option<String>,
        body: String,
    },
    PostContent {
        content: String,
        media: Vec<String>,
        visibility: String,
    },
    Navigate {
        url: String,
    },
    SaveItem {
        item_id: String,
    },
    Custom {
        action_type: String,
        payload: serde_json::Value,
    },
}

/// Result of an action
pub enum ActionResult {
    Completed { result: serde_json::Value },
    PendingApproval { action_id: String, preview: String },
    Failed { error: String },
}

/// A searchable/observable item from any platform
pub struct PlatformItem {
    pub id: String,
    pub platform: String,
    pub item_type: PlatformItemType,
    pub title: String,
    pub description: String,
    pub url: String,
    pub data: serde_json::Value,       // Platform-specific fields
    pub captured_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

pub enum PlatformItemType {
    JobListing,
    Project,
    Post,
    Profile,
    Message,
    Notification,
    Repository,
    Issue,
}

/// Search query with platform-specific filters
pub struct PlatformQuery {
    pub keywords: String,
    pub item_type: Option<PlatformItemType>,
    pub filters: serde_json::Value,    // Platform-specific filter schema
    pub limit: Option<usize>,
    pub sort: Option<String>,
}

/// Connection status for a platform
pub struct PlatformStatus {
    pub platform_id: String,
    pub accounts: Vec<AccountStatus>,
}

pub struct AccountStatus {
    pub account_id: String,        // Maps to Chrome profile
    pub profile_name: String,
    pub machine_id: String,
    pub connected: bool,
    pub last_active: Option<DateTime<Utc>>,
    pub capabilities: Vec<PlatformCapability>,
}
```

---

## 6. Browser Fleet: Multi-Machine, Multi-Account

### The Fleet Concept

Each RUSVEL instance manages a **browser fleet** — a set of Chrome profiles, each logged into a platform account, each on a specific machine.

```
┌─ Machine: macbook-m4 ──────────────────────────────────┐
│                                                         │
│  Fleet Manager (rusvel-cdp/src/fleet.rs)               │
│  │                                                      │
│  ├─ Profile: baneshi (CDP :9222)                       │
│  │  └─ Upwork (main account, logged in)                │
│  │     Platform: UpworkAdapter                         │
│  │     Status: Connected, observing                    │
│  │                                                      │
│  ├─ Profile: baneshispace (CDP :9223)                  │
│  │  └─ LinkedIn (personal)                             │
│  │     Platform: LinkedInAdapter                       │
│  │     Status: Connected, idle                         │
│  │                                                      │
│  ├─ Profile: zixlancer (CDP :9224)                     │
│  │  └─ Freelancer (zixlancer account)                  │
│  │     Platform: FreelancerAdapter                     │
│  │     Status: Connected, observing                    │
│  │                                                      │
│  ├─ Profile: ayoub (CDP :9225)                         │
│  │  └─ Upwork (second account)                         │
│  │     Platform: UpworkAdapter                         │
│  │     Status: Disconnected                            │
│  │                                                      │
│  └─ Profile: rspmedia (CDP :9226)                      │
│     └─ Twitter/X (brand account)                       │
│        Platform: TwitterAdapter                        │
│        Status: Connected, idle                         │
│                                                         │
└─────────────────────────────────────────────────────────┘

┌─ Machine: vps-eu-1 ────────────────────────────────────┐
│                                                         │
│  Fleet Manager (remote, syncs via mesh)                │
│  │                                                      │
│  ├─ Profile: leila (CDP :9222)                         │
│  │  └─ Upwork (third account, EU location)             │
│  │                                                      │
│  └─ Profile: borahan (CDP :9223)                       │
│     └─ Freelancer (second account)                     │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### Fleet Configuration

```toml
# ~/.rusvel/fleet.toml

[fleet]
machine_id = "macbook-m4"
mesh_key = "tskey-xxx"              # Tailscale/WireGuard key for mesh
coordinator = "vps-eu-1:3000"       # Central dashboard URL

[[fleet.profiles]]
id = "baneshi"
platform = "upwork"
account_label = "Main Upwork"
chrome_user_data = "~/.chrome_profiles/baneshi"
cdp_port = 9222
auto_launch = true
observe_on_start = true

[[fleet.profiles]]
id = "zixlancer"
platform = "freelancer"
account_label = "Freelancer ZIX"
chrome_user_data = "~/.chrome_profiles/zixlancer"
cdp_port = 9224
auto_launch = true
observe_on_start = true

[[fleet.profiles]]
id = "baneshispace"
platform = "linkedin"
account_label = "LinkedIn Personal"
chrome_user_data = "~/.chrome_profiles/baneshispace"
cdp_port = 9223
auto_launch = false
observe_on_start = false
```

### FleetManager

```rust
// NEW crate: rusvel-fleet (or extend rusvel-cdp)

pub struct FleetManager {
    machine_id: String,
    profiles: Vec<ManagedProfile>,
    platforms: HashMap<String, Arc<dyn PlatformPort>>,
    mesh: Option<Arc<dyn MeshTransport>>,
}

pub struct ManagedProfile {
    pub config: ChromeProfileConfig,
    pub client: Arc<CdpClient>,
    pub platform: Arc<dyn PlatformPort>,
    pub status: ProfileStatus,
    pub health: HealthState,
}

pub enum ProfileStatus {
    Stopped,
    Launching,
    Connected { since: DateTime<Utc> },
    Observing { events_captured: u64 },
    Acting { current_action: String },
    Error { message: String, since: DateTime<Utc> },
}

impl FleetManager {
    /// Launch Chrome with CDP for a profile
    pub async fn launch_profile(&self, profile_id: &str) -> Result<()>;

    /// Connect to an already-running Chrome instance
    pub async fn connect_profile(&self, profile_id: &str) -> Result<()>;

    /// Start passive observation on all connected profiles
    pub async fn start_observing_all(&self) -> Result<()>;

    /// Get the platform adapter for a specific account
    pub fn platform_for(&self, platform: &str, account_id: &str)
        -> Option<Arc<dyn PlatformPort>>;

    /// List all profiles across all machines (local + mesh)
    pub async fn all_profiles(&self) -> Vec<AccountStatus>;

    /// Health check all connections
    pub async fn health_check(&self) -> FleetHealth;
}
```

---

## 7. Passive Observation Layer

### How It Works

When a profile is "observing", RUSVEL:

1. **Enables CDP Network domain** — captures ALL HTTP responses
2. **Platform adapter filters** — recognizes platform-specific payloads
3. **Parses structured data** — extracts jobs, messages, notifications
4. **Emits PlatformEvents** — to the event bus for department consumption
5. **Stores raw captures** — for debugging and re-processing

```rust
// The observation loop (per profile)

async fn observe_loop(
    client: &CdpClient,
    platform: &dyn PlatformPort,
    event_bus: &dyn EventPort,
) {
    // Enable CDP Network domain
    client.send_cdp("Network.enable", json!({})).await;

    // Listen for responseReceived events
    let mut rx = client.subscribe_cdp_events().await;

    while let Some(cdp_event) = rx.recv().await {
        if cdp_event.method == "Network.responseReceived" {
            let url = &cdp_event.params["response"]["url"];
            let body = client.get_response_body(&cdp_event.request_id).await;

            // Let platform adapter decide if this is interesting
            if let Some(parsed) = platform.parse_network_response(url, &body) {
                // Emit as PlatformEvent
                event_bus.emit(Event::new(
                    "platform.item.discovered",
                    &parsed,
                )).await;

                // Feed to harvest engine if it's a job/opportunity
                if matches!(parsed.item_type, PlatformItemType::JobListing | PlatformItemType::Project) {
                    harvest_engine.ingest_item(&parsed).await;
                }
            }
        }
    }
}
```

### Platform-Specific Parsers

Each platform adapter knows which URLs matter and how to parse them:

```rust
// Upwork adapter — parse_network_response
impl UpworkAdapter {
    fn parse_network_response(&self, url: &str, body: &[u8]) -> Option<PlatformItem> {
        // GraphQL endpoint
        if url.contains("/api/graphql/v1") {
            return self.parse_graphql(body);
        }

        // Nuxt SSR payload
        if url.contains("/_payload.json") || url.contains("/__nuxt") {
            return self.parse_nuxt_payload(body);
        }

        // Search results API
        if url.contains("/search/jobs") {
            return self.parse_search_results(body);
        }

        // Job detail page
        if url.contains("/jobs/") && url.contains("~") {
            return self.parse_job_detail(body);
        }

        None
    }
}
```

---

## 8. Active Interaction Layer

### Actions Through the Browser

When an agent wants to act (submit proposal, send message), it goes through:

```
Agent calls platform tool
    ↓
PlatformPort::execute(action)
    ↓
Approval gate (if required by ADR-008)
    ↓
Platform adapter translates to browser actions
    ↓
BrowserPort::evaluate_js / navigate / click
    ↓
Verify result
    ↓
Return ActionResult
```

### Platform-Specific Action Scripts

```rust
// Upwork adapter — submit proposal
impl UpworkAdapter {
    async fn submit_proposal(
        &self,
        browser: &dyn BrowserPort,
        tab_id: &str,
        proposal: &SubmitProposal,
    ) -> Result<ActionResult> {
        // 1. Navigate to job page
        browser.navigate(tab_id, &format!(
            "https://www.upwork.com/jobs/{}",
            proposal.job_id
        )).await?;
        wait_human_like(2000..5000).await;

        // 2. Click "Submit a Proposal" button
        browser.evaluate_js(tab_id,
            "document.querySelector('[data-test=\"submit-proposal-btn\"]')?.click()"
        ).await?;
        wait_human_like(1500..3000).await;

        // 3. Fill cover letter
        browser.evaluate_js(tab_id, &format!(
            "document.querySelector('.cover-letter-area textarea')\
             .value = {}; \
             document.querySelector('.cover-letter-area textarea')\
             .dispatchEvent(new Event('input', {{bubbles: true}}))",
            serde_json::to_string(&proposal.cover_letter)?
        )).await?;

        // 4. Set rate if hourly
        if let Some(rate) = proposal.rate {
            browser.evaluate_js(tab_id, &format!(
                "var rateInput = document.querySelector('[data-test=\"rate-input\"]'); \
                 if (rateInput) {{ rateInput.value = '{}'; \
                 rateInput.dispatchEvent(new Event('input', {{bubbles: true}})); }}",
                rate
            )).await?;
        }

        // 5. DON'T click submit — return for human approval
        Ok(ActionResult::PendingApproval {
            action_id: uuid::Uuid::new_v4().to_string(),
            preview: format!(
                "Ready to submit proposal for '{}' at ${}/hr.\n\
                 Cover letter preview: {}...",
                proposal.job_id,
                proposal.rate.unwrap_or(0.0),
                &proposal.cover_letter[..200.min(proposal.cover_letter.len())]
            ),
        })
    }
}
```

---

## 9. Human Behavior & Security

### Anti-Detection

RUSVEL agents MUST behave like real humans browsing:

```rust
// rusvel-cdp/src/stealth.rs

pub struct HumanBehavior {
    /// Random delay between actions (not fixed intervals)
    pub action_delay: Range<u64>,     // 1500..4000ms

    /// Simulate mouse movement before clicks
    pub mouse_simulation: bool,

    /// Randomize viewport size slightly per session
    pub viewport_jitter: bool,

    /// Scroll behavior (not instant jumps)
    pub smooth_scroll: bool,

    /// Typing speed variation (not instant paste)
    pub typing_speed: Range<u64>,     // 50..150ms per character

    /// Session duration limits (don't browse 24/7)
    pub max_session_hours: f64,       // 4-8 hours, then pause

    /// Rest periods between sessions
    pub rest_hours: Range<f64>,       // 1..4 hours

    /// Time-of-day awareness (don't browse at 3am local time)
    pub active_hours: Range<u8>,      // 7..23
}

/// Wait a random human-like duration
pub async fn wait_human_like(range_ms: Range<u64>) {
    let delay = rand::thread_rng().gen_range(range_ms);
    tokio::time::sleep(Duration::from_millis(delay)).await;
}

/// Type text character by character with random delays
pub async fn type_human_like(
    browser: &dyn BrowserPort,
    tab_id: &str,
    selector: &str,
    text: &str,
    speed: Range<u64>,
) {
    for ch in text.chars() {
        browser.evaluate_js(tab_id, &format!(
            "document.querySelector('{}').value += '{}';\
             document.querySelector('{}').dispatchEvent(\
             new Event('input', {{bubbles: true}}))",
            selector, ch, selector
        )).await.ok();
        wait_human_like(speed.clone()).await;
    }
}
```

### Security Principles

1. **Profile isolation** — Each Chrome profile is a separate identity. Profiles never share cookies, storage, or fingerprint.
2. **Machine distribution** — Spread profiles across machines (different IPs, different timezones).
3. **Rate limiting** — Per-platform, per-account rate limits. Never exceed normal human activity patterns.
4. **Credential isolation** — Platform credentials stay in the browser profile. RUSVEL never extracts or stores passwords.
5. **Approval gates** — All outbound actions (proposals, messages, posts) require human approval by default. Auto-approve only below configured thresholds.
6. **Audit trail** — Every action logged to event bus. Every capture stored. Full traceability.

---

## 10. Cloud Coordination (Mesh)

### Mesh Architecture

RUSVEL instances across machines form a mesh network (inspired by Tailscale):

```
┌──────────────────┐         ┌──────────────────┐
│  macbook-m4      │◄───────►│  vps-eu-1        │
│  RUSVEL instance │  mesh   │  RUSVEL instance  │
│  + 5 profiles    │ (WG/TS) │  + 2 profiles    │
│  + local SQLite  │         │  + local SQLite   │
└────────┬─────────┘         └────────┬──────────┘
         │                            │
         └──────────┬─────────────────┘
                    │
           ┌────────▼────────┐
           │  Coordinator    │
           │  (any node, or  │
           │   dedicated)    │
           │                 │
           │  Merged view    │
           │  Approval queue │
           │  Dashboard      │
           └─────────────────┘
```

### MeshTransport Port

```rust
// rusvel-core/src/ports.rs — NEW

#[async_trait]
pub trait MeshPort: Send + Sync {
    /// This node's identity
    fn node_id(&self) -> &str;

    /// Discover other RUSVEL nodes on the mesh
    async fn discover_peers(&self) -> Result<Vec<PeerInfo>>;

    /// Push local state to peers (opportunities, events, scores)
    async fn push(&self, payload: MeshPayload) -> Result<()>;

    /// Pull state from peers since a timestamp
    async fn pull(&self, since: DateTime<Utc>) -> Result<MeshPayload>;

    /// Forward an action to a specific node (e.g., "submit proposal on vps-eu-1")
    async fn forward_action(&self, node_id: &str, action: PlatformAction)
        -> Result<ActionResult>;
}

pub struct PeerInfo {
    pub node_id: String,
    pub address: String,          // Tailscale IP or WireGuard endpoint
    pub profiles: Vec<String>,    // Which browser profiles this node manages
    pub last_seen: DateTime<Utc>,
}

pub struct MeshPayload {
    pub from_node: String,
    pub opportunities: Vec<PlatformItem>,
    pub events: Vec<PlatformEvent>,
    pub scores: Vec<CompositeScore>,
    pub timestamp: DateTime<Utc>,
}
```

---

## 11. Integration with Every Department

### Platform Tools per Department

The god agent and department agents get platform tools registered dynamically based on which platforms are connected:

| Department | Platform Tools | Use Case |
|-----------|---------------|----------|
| **Harvest** | `platform_search_jobs`, `platform_get_job`, `platform_save_job` | Discover and track opportunities |
| **GTM** | `platform_send_message`, `platform_submit_proposal`, `platform_read_messages` | Outreach, proposals, follow-ups |
| **Content** | `platform_post_content`, `platform_read_analytics` | Publish to LinkedIn/Twitter/DEV.to |
| **Code** | `platform_get_repo`, `platform_create_issue`, `platform_read_issues` | GitHub integration |
| **Infra** | `platform_read_notifications` | Monitor deployment status on cloud dashboards |
| **Finance** | `platform_read_earnings`, `platform_create_invoice` | Track platform earnings |
| **Legal** | `platform_read_contract`, `platform_read_terms` | Review client contracts |
| **Support** | `platform_read_reviews`, `platform_respond_review` | Manage client reviews/feedback |
| **Forge** | All of the above via delegation | God agent orchestrates cross-platform |

### Cross-Department Playbook Example

```
"Win & Deliver" Playbook (triggered when Won detected):

1. HARVEST: Extract full job details from platform
2. CODE: Clone client repo (if GitHub link in job)
3. CODE: Scaffold project structure
4. INFRA: Deploy staging environment
5. LEGAL: Generate contract from job terms
6. FINANCE: Create milestone invoice schedule
7. GTM: Send "excited to start" message to client on platform
8. FORGE: Create daily standup schedule + goals
9. CONTENT: Draft "new project" LinkedIn post
```

---

## 12. Auto-Capability: Agents Equip Themselves

When a platform event triggers a new opportunity type the system hasn't seen before, the capability engine auto-equips:

```
PlatformEvent: New job "Build a dental appointment chatbot with Twilio"
    ↓
God Agent analyzes:
    "This needs: Twilio API, healthcare compliance, chatbot patterns"
    ↓
Capability Engine:
    1. Search for Twilio MCP server → install
    2. Create "dental-terminology" skill → install to harvest dept
    3. Create "HIPAA compliance" rule → install to code dept
    4. Create "chatbot-architecture" workflow → install to code dept
    5. Create "healthcare-proposal" agent persona → install to harvest dept
    ↓
All departments now equipped for this job type
    ↓
Next similar job: already equipped, skip capability creation
```

This makes RUSVEL a **learning system** — it gets better at handling specific job categories over time because the auto-created capabilities persist.

---

## 13. Domain Model

### New Types Summary

```rust
// rusvel-core/src/domain.rs — additions

// Platform identity
pub struct PlatformAccount {
    pub platform: String,          // "upwork", "freelancer", "linkedin"
    pub account_id: String,        // Maps to Chrome profile ID
    pub account_label: String,     // Human-readable: "Main Upwork"
    pub machine_id: String,        // Which RUSVEL node manages this
    pub metadata: serde_json::Value,
}

// Captured item from any platform
pub struct PlatformItem { ... }        // See section 5
pub enum PlatformItemType { ... }      // See section 5
pub enum PlatformEvent { ... }         // See section 5
pub enum PlatformAction { ... }        // See section 5
pub enum PlatformCapability { ... }    // See section 5
pub enum ActionResult { ... }          // See section 5
pub struct PlatformQuery { ... }       // See section 5
pub struct PlatformStatus { ... }      // See section 5
pub struct AccountStatus { ... }       // See section 5

// Browser fleet
pub struct FleetConfig { ... }         // See section 6
pub struct ManagedProfile { ... }      // See section 6

// Mesh coordination
pub struct PeerInfo { ... }            // See section 10
pub struct MeshPayload { ... }         // See section 10

// Human behavior config
pub struct HumanBehavior { ... }       // See section 9
```

---

## 14. Port & Trait Design

### New Ports

| Port | Location | Responsibility |
|------|----------|---------------|
| `PlatformPort` | `rusvel-core/src/ports.rs` | Platform-agnostic access to any online service |
| `MeshPort` | `rusvel-core/src/ports.rs` | Cross-machine coordination and state sync |

### Extended Ports

| Port | Addition | Reason |
|------|----------|--------|
| `BrowserPort` | `launch(profile)`, `close(profile)`, `screenshot(tab)` | Fleet needs to manage Chrome lifecycle |
| `EventPort` | Platform event kinds in existing string-based system | No port change needed — just new event kind strings |

### New Adapter Crates

| Crate | Port | Transport |
|-------|------|-----------|
| `rusvel-platform-upwork` | `PlatformPort` | CDP via BrowserPort |
| `rusvel-platform-freelancer` | `PlatformPort` | CDP via BrowserPort |
| `rusvel-platform-linkedin` | `PlatformPort` | CDP via BrowserPort |
| `rusvel-platform-github` | `PlatformPort` | REST API (no browser needed) |
| `rusvel-platform-twitter` | `PlatformPort` | CDP via BrowserPort |
| `rusvel-mesh` | `MeshPort` | WireGuard/Tailscale + HTTP |

Or, to keep crate count manageable:

| Crate | Contents |
|-------|----------|
| `rusvel-platforms` | All platform adapters (each < 500 lines, total < 2000) |
| `rusvel-fleet` | FleetManager + Chrome lifecycle + stealth |
| `rusvel-mesh` | MeshPort adapter + sync protocol |

### Hexagonal Compliance

```
Engines (harvest, content, gtm, code, infra)
    │
    ▼ depend ONLY on
rusvel-core (PlatformPort, BrowserPort, MeshPort traits)
    ▲
    │ implemented by
Adapters (rusvel-platforms, rusvel-fleet, rusvel-mesh, rusvel-cdp)
    │
    ▼ wired in
rusvel-app (composition root)
```

No engine imports an adapter crate. All through ports. ADR-009 compliant.

---

## 15. Implementation Phases

### Phase 0: NetworkCapture (the foundation) — 3-5 days

The `NetworkCapture` stub must become real. Without it, nothing else works.

| Task | Effort | File |
|------|--------|------|
| Implement `Network.enable` via CDP WebSocket | 4h | `rusvel-cdp/src/network.rs` |
| Parse `Network.responseReceived` events | 4h | `rusvel-cdp/src/network.rs` |
| `GetResponseBody` for captured requests | 3h | `rusvel-cdp/src/network.rs` |
| Wire captures to `BrowserEvent::DataCaptured` broadcast | 2h | `rusvel-cdp/src/lib.rs` |
| Upwork: recognize `__NUXT__`, GraphQL, search URLs | 4h | `rusvel-cdp/src/platforms/upwork.rs` |
| Freelancer: recognize ngrx, REST API URLs | 4h | `rusvel-cdp/src/platforms/freelancer.rs` (new) |
| Parse captured payloads → structured `PlatformItem` | 6h | Platform modules |
| Tests: mock CDP events → parsed items | 4h | Tests |

### Phase 1: PlatformPort + First Adapters — 5-8 days

| Task | Effort | File |
|------|--------|------|
| `PlatformPort` trait + domain types in `rusvel-core` | 4h | `rusvel-core/src/ports.rs`, `domain.rs` |
| `rusvel-platforms` crate with Upwork adapter | 8h | New crate |
| Freelancer adapter | 6h | `rusvel-platforms/src/freelancer.rs` |
| Platform tools for agents (`platform_search`, `platform_act`, etc.) | 6h | `rusvel-builtin-tools` or `rusvel-engine-tools` |
| Wire into harvest engine (replace mock/RSS with platform) | 4h | `harvest-engine` |
| Wire into god agent capabilities overview | 2h | `rusvel-api/src/chat.rs` |
| Extend `CdpSource` to use `PlatformPort` | 3h | `harvest-engine/src/cdp_source.rs` |

### Phase 2: Fleet Manager — 5-8 days

| Task | Effort | File |
|------|--------|------|
| `FleetManager` with Chrome launch/connect/health | 8h | `rusvel-fleet` or `rusvel-cdp/src/fleet.rs` |
| Fleet config (`fleet.toml`) loading | 3h | `rusvel-config` |
| Auto-launch configured profiles on startup | 3h | `rusvel-app/src/main.rs` |
| Fleet API endpoints (`/api/fleet/*`) | 4h | `rusvel-api` |
| Fleet dashboard UI (profile status, events, controls) | 6h | `frontend` |
| Human behavior module (delays, typing, scrolling) | 4h | `rusvel-cdp/src/stealth.rs` |

### Phase 3: Active Interactions — 5-8 days

| Task | Effort | File |
|------|--------|------|
| Upwork: submit proposal (fill form, approval gate) | 8h | `rusvel-platforms/src/upwork.rs` |
| Upwork: send message | 4h | `rusvel-platforms/src/upwork.rs` |
| Freelancer: submit bid | 6h | `rusvel-platforms/src/freelancer.rs` |
| LinkedIn: post content | 4h | `rusvel-platforms/src/linkedin.rs` (new) |
| GitHub: create issue, PR review | 4h | `rusvel-platforms/src/github.rs` (new, REST API) |
| Approval queue integration | 3h | `rusvel-api/src/approvals.rs` |
| Cross-department playbooks using platform actions | 4h | `rusvel-api/src/playbooks.rs` |

### Phase 4: Mesh & Cloud Dashboard — 8-12 days

| Task | Effort | File |
|------|--------|------|
| `MeshPort` trait | 2h | `rusvel-core/src/ports.rs` |
| `rusvel-mesh` crate (HTTP sync over WireGuard/Tailscale) | 8h | New crate |
| Peer discovery (multicast or config-based) | 4h | `rusvel-mesh` |
| State sync protocol (push/pull with conflict resolution) | 8h | `rusvel-mesh` |
| Cross-machine action forwarding | 4h | `rusvel-mesh` |
| Coordinator mode (merged dashboard) | 8h | `rusvel-api` + `frontend` |
| Remote fleet management UI | 6h | `frontend` |

### Phase 5: Learning & Auto-Equip — 5-8 days

| Task | Effort | File |
|------|--------|------|
| Outcome recording per platform | 3h | `harvest-engine` |
| Scorer weight updates from outcomes | 4h | `harvest-engine` |
| Auto-capability trigger on high-score jobs | 4h | `rusvel-api/src/capability.rs` |
| Capability caching (don't re-create for similar jobs) | 3h | `rusvel-api/src/capability.rs` |
| Cross-department "Win & Deliver" playbook | 4h | `rusvel-api/src/playbooks.rs` |
| Platform-specific playbooks (Upwork workflow, Freelancer workflow) | 4h | `dept-harvest` |

---

## 16. Open Questions for Research

Before implementation, these questions need answers. Each could be a research task:

### Architecture

1. **Should platform adapters be one crate or many?** One `rusvel-platforms` (under 2000 lines?) or separate `rusvel-platform-upwork`, `rusvel-platform-freelancer` (cleaner separation, more crates)?

2. **Should FleetManager be a port trait or an adapter internal?** Does any engine need to interact with the fleet directly, or is it always mediated by PlatformPort?

3. **How does MeshPort relate to existing EventPort?** Should mesh sync use the event bus, or is it a separate concern?

### Technical

4. **CDP WebSocket management at scale** — How do we maintain 10+ WebSocket connections efficiently? tokio task per connection? Connection pool patterns?

5. **Chrome launch automation** — How to launch Chrome with specific profile from Rust? `std::process::Command` with `--remote-debugging-port` and `--user-data-dir`?

6. **Anti-detection state of the art (2026)** — What does Upwork/LinkedIn detect? Headless detection, automation flags, behavior analysis? Search for: `puppeteer-extra-plugin-stealth` Rust equivalent, `undetected-chromedriver` patterns.

7. **Tailscale SDK for Rust** — Does a Rust client exist? Or do we shell out to `tailscale` CLI? Alternative: WireGuard via `boringtun` crate?

### Product

8. **Approval UX for mobile** — How should the approval queue look on a phone? Push notification → approve in 1 tap?

9. **Dashboard information architecture** — What does the cloud dashboard show? Per-machine view? Per-platform view? Pipeline view? All three?

10. **Rate limits by platform** — What are Upwork/Freelancer/LinkedIn's actual rate limits for browsing, messaging, proposals? How do we stay under them?

### Prompts for External Research

**For Perplexity/Claude:**
```
What is the current state of browser automation anti-detection in 2026?
Specifically for Upwork and LinkedIn. What signals do they check?
How does puppeteer-extra-plugin-stealth work and what is the Rust equivalent?
Are there any Rust crates for CDP stealth browsing?
```

```
What MCP servers exist for browser automation as of March 2026?
Specifically: playwright-mcp, puppeteer-mcp, browser-use MCP.
What tools do they expose? How do they handle authentication?
Can a Chrome extension act as an MCP server?
```

```
How does Tailscale's mesh networking work at the protocol level?
Is there a Rust SDK or do you shell out to the CLI?
What are the alternatives for building a lightweight mesh
between 3-5 machines for state synchronization?
```

```
What is the latest on Anthropic's computer-use API as of 2026?
Can Claude take screenshots and interact with browsers directly?
How does this compare to CDP-based browser control?
What is the AG-UI protocol and how does it work with browser agents?
```

---

## Summary

This is not a feature request. This is the **core capability** that makes RUSVEL a virtual agency instead of a chatbot. Every department's value proposition depends on being able to see and act on the internet:

- Harvest can't find jobs without browser observation
- Content can't publish without platform access
- GTM can't do outreach without messaging capability
- Code can't onboard clients without GitHub access
- Forge can't orchestrate without knowing what's happening across platforms

The single-binary rule holds. Everything is Rust crates inside RUSVEL. The only external pieces are Chrome instances (which are the user's own browsers) and optionally a mesh transport (Tailscale/WireGuard).

**Start with Phase 0 (NetworkCapture).** Without real CDP network interception, everything else is academic. Once RUSVEL can passively see what's happening in a logged-in browser, the rest follows naturally.
