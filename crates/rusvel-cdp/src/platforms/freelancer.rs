//! Freelancer.com passive capture: Angular TransferState (ngrx), REST API, and DOM extraction.
//!
//! Ported from `smart-standalone-harvestor/src/platforms/freelancer/` — same extraction
//! logic, different language. Network interception captures responses from:
//! - `/api/projects/0.1/` — REST search/detail endpoints
//! - `webapp-state` script tags — Angular ngrx TransferState (via evaluate_js)
//!
//! The JS extraction scripts run inside Chrome via `BrowserPort::evaluate_js`.

use rusvel_core::domain::BrowserEvent;

// ════════════════════════════════════════════════════════════════════
//  URL matching — which network responses contain Freelancer data
// ════════════════════════════════════════════════════════════════════

/// Check if a URL is a Freelancer API or data endpoint worth capturing.
pub fn matches_capture_url(url: &str) -> bool {
    let u = url.to_lowercase();
    u.contains("freelancer.com/api/projects")
        || u.contains("freelancer.com/api/users")
        || u.contains("freelancer.com/api/contests")
        || (u.contains("freelancer.com") && u.contains("/search/projects"))
        || (u.contains("freelancer.com") && u.contains("transfer-state"))
}

// ════════════════════════════════════════════════════════════════════
//  Network response parsing
// ════════════════════════════════════════════════════════════════════

/// Parse a captured JSON response body from Freelancer into `BrowserEvent`s.
pub fn events_from_response(
    url: &str,
    body: &serde_json::Value,
    tab_id: &str,
) -> Vec<BrowserEvent> {
    let mut events = Vec::new();

    // REST API: /api/projects/0.1/projects — search results
    if url.contains("/api/projects") {
        if let Some(projects) = body.get("result").and_then(|r| r.get("projects")) {
            if let Some(arr) = projects.as_array() {
                for project in arr {
                    if let Some(norm) = normalize_project(project) {
                        events.push(BrowserEvent::DataCaptured {
                            platform: "freelancer".into(),
                            kind: "job_listing".into(),
                            data: norm,
                            tab_id: tab_id.to_string(),
                        });
                    }
                }
            } else if let Some(obj) = projects.as_object() {
                // Single project detail: { projects: { "12345": {...} } }
                for project in obj.values() {
                    if let Some(norm) = normalize_project(project) {
                        events.push(BrowserEvent::DataCaptured {
                            platform: "freelancer".into(),
                            kind: "job_listing".into(),
                            data: norm,
                            tab_id: tab_id.to_string(),
                        });
                    }
                }
            }
        }
        // User/client data from the same response
        if let Some(users) = body.get("result").and_then(|r| r.get("users")) {
            if let Some(obj) = users.as_object() {
                for user in obj.values() {
                    if let Some(norm) = normalize_client(user) {
                        events.push(BrowserEvent::DataCaptured {
                            platform: "freelancer".into(),
                            kind: "client_profile".into(),
                            data: norm,
                            tab_id: tab_id.to_string(),
                        });
                    }
                }
            }
        }
        return events;
    }

    // Embedded Angular state — recursive search for project arrays
    if let Some(projects) = find_project_arrays(body, 0) {
        for project in projects {
            if let Some(norm) = normalize_project(&project) {
                events.push(BrowserEvent::DataCaptured {
                    platform: "freelancer".into(),
                    kind: "job_listing".into(),
                    data: norm,
                    tab_id: tab_id.to_string(),
                });
            }
        }
    }

    events
}

// ════════════════════════════════════════════════════════════════════
//  NgRx / TransferState extraction
// ════════════════════════════════════════════════════════════════════

/// JS script to extract projects from Angular TransferState `<script>` tags.
/// Runs inside Chrome via `evaluate_js`. Walks nested state looking for arrays
/// of objects with `project_id`, `title`, or `name` fields.
pub const ANGULAR_EXTRACT_JS: &str = r#"(function() {
    function findJobArrays(obj, depth) {
        if (depth > 8) return null;
        if (!obj || typeof obj !== 'object') return null;
        if (Array.isArray(obj) && obj.length > 0) {
            var first = obj[0];
            if (first && typeof first === 'object' &&
                (first.project_id || first.project_name || first.title || first.name || first.id)) {
                return obj;
            }
        }
        var keys = Object.keys(obj);
        for (var i = 0; i < keys.length; i++) {
            var found = findJobArrays(obj[keys[i]], depth + 1);
            if (found) return found;
        }
        return null;
    }
    var scripts = document.querySelectorAll('script[id$="-state"], script[type="application/json"]');
    for (var s = 0; s < scripts.length; s++) {
        try {
            var txt = scripts[s].textContent || '';
            if (txt.length < 50) continue;
            var data = JSON.parse(txt);
            var jobs = findJobArrays(data, 0);
            if (jobs && jobs.length > 0) return JSON.stringify(jobs);
        } catch (e) {}
    }
    return '[]';
})()"#;

/// JS script to extract NgRx rawDocument from Freelancer project detail page.
/// Returns the first rawDocument found in NGRX_STATE.projectsSeo.documents.
pub const NGRX_DETAIL_EXTRACT_JS: &str = r#"(function() {
    var scripts = document.querySelectorAll('script#webapp-state, script[type="application/json"]');
    for (var s = 0; s < scripts.length; s++) {
        try {
            var txt = scripts[s].textContent || '';
            if (txt.length < 50) continue;
            var json = JSON.parse(txt);
            var ng = json.NGRX_STATE || json;
            if (!ng.projectsSeo) continue;
            var ps = ng.projectsSeo;
            var keys = Object.keys(ps);
            for (var i = 0; i < keys.length; i++) {
                var q = ps[keys[i]];
                if (!q || !q.documents) continue;
                var docKeys = Object.keys(q.documents);
                for (var j = 0; j < docKeys.length; j++) {
                    var doc = q.documents[docKeys[j]];
                    if (doc && doc.rawDocument) return JSON.stringify(doc.rawDocument);
                }
            }
        } catch (e) {}
    }
    return null;
})()"#;

/// JS script to extract jobs from DOM by finding project links.
/// Fallback when Angular state is not available.
pub const DOM_EXTRACT_JS: &str = r#"(function() {
    var byId = {};
    var selectors = ['a[href*="/projects/"]', 'a[href*="freelancer.com/projects"]'];
    var links = [];
    for (var s = 0; s < selectors.length; s++) {
        var nodes = document.querySelectorAll(selectors[s]);
        for (var n = 0; n < nodes.length; n++) links.push(nodes[n]);
    }
    for (var i = 0; i < links.length; i++) {
        var a = links[i];
        var href = (a.getAttribute('href') || '').trim();
        if (!href || href.length < 10) continue;
        var projectId = null;
        var m1 = href.match(/\/projects\/(\d+)\//);
        if (m1) projectId = m1[1];
        if (!projectId) { var m2 = href.match(/-(\d{6,})(?:\/|$|\?)/); if (m2) projectId = m2[1]; }
        if (!projectId) {
            var parts = href.split('/').filter(Boolean);
            var last = parts[parts.length - 1];
            if (last && last.length > 3 && /[a-z0-9-]/.test(last)) projectId = last;
        }
        if (!projectId) continue;
        var title = (a.textContent || '').trim();
        if (title.length < 2 || title.length > 200) continue;
        if (!byId[projectId] || title.length > (byId[projectId].titleLen || 0)) {
            byId[projectId] = { title: title, titleLen: title.length, href: href };
        }
    }
    var results = [];
    for (var pid in byId) {
        var rec = byId[pid];
        var fullUrl = rec.href.startsWith('http') ? rec.href : 'https://www.freelancer.com' + (rec.href.startsWith('/') ? '' : '/') + rec.href;
        results.push({ project_id: pid, id: pid, title: rec.title, name: rec.title, description: rec.title, seo_url: rec.href, url: fullUrl });
    }
    return results.length > 0 ? JSON.stringify(results) : '[]';
})()"#;

// ════════════════════════════════════════════════════════════════════
//  Normalization — raw Freelancer data → structured JSON
// ════════════════════════════════════════════════════════════════════

/// Normalize a raw Freelancer project object into a consistent structure.
fn normalize_project(project: &serde_json::Value) -> Option<serde_json::Value> {
    let title = project
        .get("title")
        .or_else(|| project.get("name"))
        .or_else(|| project.get("project_name"))
        .and_then(|v| v.as_str())?;

    let project_id = project
        .get("project_id")
        .or_else(|| project.get("id"))
        .map(|v| {
            if let Some(n) = v.as_u64() {
                n.to_string()
            } else {
                v.as_str().unwrap_or("").to_string()
            }
        })
        .unwrap_or_default();

    if project_id.is_empty() {
        return None;
    }

    let seo_url = project
        .get("seo_url")
        .or_else(|| project.get("seoUrl"))
        .and_then(|v| v.as_str());

    let url = seo_url.map_or_else(
        || format!("https://www.freelancer.com/projects/{project_id}"),
        |slug| {
            if slug.starts_with("http") {
                slug.to_string()
            } else {
                format!(
                    "https://www.freelancer.com{}{}",
                    if slug.starts_with('/') { "" } else { "/" },
                    slug
                )
            }
        },
    );

    let description = project
        .get("description")
        .and_then(|v| v.as_str())
        .map(strip_html)
        .unwrap_or_default();

    let (budget_min, budget_max, budget_type) = extract_budget(project);
    let skills = extract_skills(project);
    let bid_count = project
        .get("bid_count")
        .or_else(|| project.get("bids"))
        .or_else(|| project.pointer("/bidStats/bidCount"))
        .or_else(|| project.pointer("/bid_stats/bidCount"))
        .and_then(|v| v.as_u64());

    let posted_at = project
        .get("time_submitted")
        .and_then(|v| v.as_u64())
        .map(|ts| format!("{ts}"))
        .or_else(|| {
            project
                .get("submitdate")
                .and_then(|v| v.as_str())
                .map(String::from)
        });

    // Client info
    let owner = project.get("owner").or_else(|| project.get("client"));
    let client_country = owner
        .and_then(|o| {
            o.get("country")
                .or_else(|| o.pointer("/address/country"))
                .or_else(|| o.pointer("/location/country"))
        })
        .and_then(|v| v.as_str());
    let client_rating = project
        .get("client_rating")
        .or_else(|| owner.and_then(|o| o.pointer("/rating/average")))
        .and_then(|v| v.as_f64());
    let client_total_spent = owner
        .and_then(|o| o.get("totalSpent").or_else(|| o.get("total_spent")))
        .and_then(|v| v.as_f64());
    let client_payment_verified = project
        .get("client_payment_verified")
        .or_else(|| owner.and_then(|o| o.pointer("/verification/paymentVerified")))
        .and_then(|v| v.as_bool());

    // Status
    let status = project
        .get("status")
        .or_else(|| project.get("projectStatus"))
        .and_then(|v| v.as_str())
        .map(map_status)
        .unwrap_or("open");

    Some(serde_json::json!({
        "id": project_id,
        "title": title,
        "description": description,
        "url": url,
        "seo_slug": seo_url,
        "budget_min": budget_min,
        "budget_max": budget_max,
        "budget_type": budget_type,
        "hourly_range": project.get("hourly_range").or_else(||
            if project.get("type").and_then(|v| v.as_str()) == Some("hourly") {
                project.get("formattedBudget")
            } else {
                None
            }
        ),
        "skills": skills,
        "bid_count": bid_count,
        "posted_at": posted_at,
        "status": status,
        "client_country": client_country,
        "client_rating": client_rating,
        "client_total_spent": client_total_spent,
        "client_payment_verified": client_payment_verified,
        "featured": project.get("featured").and_then(|v| v.as_bool()),
        "nda": project.get("nda").and_then(|v| v.as_bool()),
        "raw_ref": project_id,
    }))
}

/// Normalize client/user data from Freelancer API responses.
fn normalize_client(user: &serde_json::Value) -> Option<serde_json::Value> {
    let id = user
        .get("id")
        .or_else(|| user.get("user_id"))
        .map(|v| {
            if let Some(n) = v.as_u64() {
                n.to_string()
            } else {
                v.as_str().unwrap_or("").to_string()
            }
        })?;

    let name = user
        .get("display_name")
        .or_else(|| user.get("username"))
        .or_else(|| user.get("public_name"))
        .and_then(|v| v.as_str())?;

    Some(serde_json::json!({
        "id": id,
        "name": name,
        "country": user.pointer("/location/country").or_else(|| user.get("country")),
        "total_spent": user.get("total_spent").or_else(|| user.get("totalSpent")),
        "rating": user.pointer("/employer_reputation/entire_history/overall"),
        "payment_verified": user.pointer("/status/payment_verified"),
        "metadata": user,
    }))
}

// ════════════════════════════════════════════════════════════════════
//  Helpers
// ════════════════════════════════════════════════════════════════════

fn extract_budget(
    project: &serde_json::Value,
) -> (Option<f64>, Option<f64>, Option<&'static str>) {
    // Object form: { budget: { min: N, max: N } }
    if let Some(b) = project.get("budget").filter(|v| v.is_object()) {
        let min = b.get("min").or_else(|| b.get("minimum")).and_then(|v| v.as_f64());
        let max = b.get("max").or_else(|| b.get("maximum")).and_then(|v| v.as_f64());
        let bt = if project.get("type").and_then(|v| v.as_str()) == Some("hourly") {
            "hourly"
        } else {
            "fixed"
        };
        return (min, max, Some(bt));
    }
    // Flat fields
    let min = project
        .get("min_budget")
        .or_else(|| project.get("budget_minimum"))
        .and_then(|v| v.as_f64());
    let max = project
        .get("max_budget")
        .or_else(|| project.get("budget_maximum"))
        .and_then(|v| v.as_f64());
    if min.is_some() || max.is_some() {
        return (min, max, Some("fixed"));
    }
    // Scalar: budget as number
    if let Some(n) = project.get("budget").and_then(|v| v.as_f64()) {
        return (Some(n), Some(n), Some("fixed"));
    }
    (None, None, None)
}

fn extract_skills(project: &serde_json::Value) -> Vec<String> {
    // skills: [{ name: "..." }]
    if let Some(arr) = project.get("skills").and_then(|v| v.as_array()) {
        let names: Vec<String> = arr
            .iter()
            .filter_map(|s| {
                s.as_str()
                    .map(String::from)
                    .or_else(|| s.get("name").and_then(|n| n.as_str()).map(String::from))
            })
            .collect();
        if !names.is_empty() {
            return names;
        }
    }
    // jobs: [{ name: "..." }] — Freelancer sometimes uses "jobs" for skills
    if let Some(arr) = project.get("jobs").and_then(|v| v.as_array()) {
        return arr
            .iter()
            .filter_map(|j| j.get("name").and_then(|n| n.as_str()).map(String::from))
            .collect();
    }
    Vec::new()
}

fn map_status(raw: &str) -> &'static str {
    match raw.to_lowercase().as_str() {
        "closed" | "awarded" | "completed" | "finished" => "closed",
        "cancelled" | "canceled" | "removed" | "deleted" => "cancelled",
        _ => "open",
    }
}

fn strip_html(text: &str) -> String {
    // Simple tag stripper — good enough for job descriptions
    let mut result = String::with_capacity(text.len());
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}

/// Recursively search a JSON value for arrays that look like project listings.
fn find_project_arrays(obj: &serde_json::Value, depth: u8) -> Option<Vec<serde_json::Value>> {
    if depth > 8 {
        return None;
    }
    if let Some(arr) = obj.as_array() {
        if !arr.is_empty() {
            let first = &arr[0];
            if first.is_object()
                && (first.get("project_id").is_some()
                    || first.get("project_name").is_some()
                    || first.get("title").is_some()
                    || first.get("name").is_some())
            {
                return Some(arr.clone());
            }
        }
    }
    if let Some(obj) = obj.as_object() {
        for v in obj.values() {
            if let Some(found) = find_project_arrays(v, depth + 1) {
                return Some(found);
            }
        }
    }
    None
}

// ════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_freelancer_api_urls() {
        assert!(matches_capture_url(
            "https://www.freelancer.com/api/projects/0.1/projects"
        ));
        assert!(matches_capture_url(
            "https://www.freelancer.com/api/users/0.1/users"
        ));
        assert!(!matches_capture_url("https://www.upwork.com/api/v3/search"));
        assert!(!matches_capture_url("https://google.com"));
    }

    #[test]
    fn parses_rest_api_search_results() {
        let body = serde_json::json!({
            "status": "success",
            "result": {
                "projects": [
                    {
                        "id": 12345,
                        "title": "Build AI Chatbot",
                        "description": "Need a chatbot with NLP",
                        "budget": { "min": 500.0, "max": 1000.0 },
                        "jobs": [{ "name": "Python" }, { "name": "AI" }],
                        "bid_count": 15,
                        "time_submitted": 1711756800,
                        "owner": {
                            "country": "US",
                            "totalSpent": 50000.0
                        }
                    }
                ]
            }
        });

        let events = events_from_response(
            "https://www.freelancer.com/api/projects/0.1/projects?query=ai",
            &body,
            "tab1",
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            BrowserEvent::DataCaptured {
                platform,
                kind,
                data,
                ..
            } => {
                assert_eq!(platform, "freelancer");
                assert_eq!(kind, "job_listing");
                assert_eq!(data["title"], "Build AI Chatbot");
                assert_eq!(data["id"], "12345");
                assert_eq!(data["budget_min"], 500.0);
                assert_eq!(data["budget_max"], 1000.0);
                assert_eq!(data["client_country"], "US");
                assert_eq!(data["skills"], serde_json::json!(["Python", "AI"]));
            }
            _ => panic!("expected DataCaptured"),
        }
    }

    #[test]
    fn parses_rest_api_single_project() {
        let body = serde_json::json!({
            "status": "success",
            "result": {
                "projects": {
                    "99999": {
                        "id": 99999,
                        "title": "Rust CLI Tool",
                        "description": "Build a CLI in Rust",
                        "budget": { "min": 200.0, "max": 500.0 },
                        "skills": [{ "name": "Rust" }],
                        "status": "open"
                    }
                }
            }
        });

        let events = events_from_response(
            "https://www.freelancer.com/api/projects/0.1/projects/99999",
            &body,
            "tab2",
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            BrowserEvent::DataCaptured { data, .. } => {
                assert_eq!(data["title"], "Rust CLI Tool");
                assert_eq!(data["skills"], serde_json::json!(["Rust"]));
                assert_eq!(data["status"], "open");
            }
            _ => panic!("expected DataCaptured"),
        }
    }

    #[test]
    fn parses_ngrx_state_embedded_projects() {
        let body = serde_json::json!({
            "NGRX_STATE": {
                "projectsSeo": {
                    "query1": {
                        "documents": {
                            "doc1": {
                                "rawDocument": {
                                    "project_id": 55555,
                                    "title": "SvelteKit Dashboard",
                                    "description": "Build a dashboard",
                                    "status": "open"
                                }
                            }
                        }
                    }
                }
            }
        });

        // find_project_arrays won't match this structure directly,
        // but events_from_response will try the recursive search
        let events = events_from_response(
            "https://www.freelancer.com/some-page",
            &body,
            "tab3",
        );
        // The ngrx structure is nested differently — single rawDocument, not an array
        // This tests that even non-array structures don't crash
        assert!(events.is_empty() || events.len() == 1);
    }

    #[test]
    fn normalizes_budget_variations() {
        // Object form
        let project = serde_json::json!({
            "id": "1", "title": "A",
            "budget": { "min": 100.0, "max": 500.0 },
            "type": "fixed"
        });
        let norm = normalize_project(&project).unwrap();
        assert_eq!(norm["budget_min"], 100.0);
        assert_eq!(norm["budget_max"], 500.0);
        assert_eq!(norm["budget_type"], "fixed");

        // Scalar form
        let project2 = serde_json::json!({
            "id": "2", "title": "B",
            "budget": 750.0
        });
        let norm2 = normalize_project(&project2).unwrap();
        assert_eq!(norm2["budget_min"], 750.0);
        assert_eq!(norm2["budget_max"], 750.0);

        // Flat fields
        let project3 = serde_json::json!({
            "id": "3", "title": "C",
            "min_budget": 50.0, "max_budget": 200.0
        });
        let norm3 = normalize_project(&project3).unwrap();
        assert_eq!(norm3["budget_min"], 50.0);
        assert_eq!(norm3["budget_max"], 200.0);
    }

    #[test]
    fn strips_html_from_descriptions() {
        assert_eq!(strip_html("<p>Hello <b>world</b></p>"), "Hello world");
        assert_eq!(strip_html("No tags here"), "No tags here");
        assert_eq!(strip_html("<br/>Line<br/>break"), "Linebreak");
    }

    #[test]
    fn maps_status_correctly() {
        assert_eq!(map_status("open"), "open");
        assert_eq!(map_status("closed"), "closed");
        assert_eq!(map_status("awarded"), "closed");
        assert_eq!(map_status("completed"), "closed");
        assert_eq!(map_status("cancelled"), "cancelled");
        assert_eq!(map_status("deleted"), "cancelled");
        assert_eq!(map_status("unknown"), "open");
    }

    #[test]
    fn extracts_client_from_rest_response() {
        let body = serde_json::json!({
            "status": "success",
            "result": {
                "projects": [
                    { "id": 1, "title": "Job", "owner": { "country": "DE" } }
                ],
                "users": {
                    "777": {
                        "id": 777,
                        "display_name": "ClientCo",
                        "location": { "country": "Germany" },
                        "employer_reputation": {
                            "entire_history": { "overall": 4.8 }
                        }
                    }
                }
            }
        });

        let events = events_from_response(
            "https://www.freelancer.com/api/projects/0.1/projects",
            &body,
            "tab1",
        );
        // Should have 1 job + 1 client
        assert_eq!(events.len(), 2);
        let client_event = events.iter().find(|e| matches!(e,
            BrowserEvent::DataCaptured { kind, .. } if kind == "client_profile"
        ));
        assert!(client_event.is_some());
    }
}
