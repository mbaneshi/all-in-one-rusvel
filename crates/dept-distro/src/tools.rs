//! Agent tools for the Distribution department.

use std::sync::Arc;

use distro_engine::DistroEngine;
use rusvel_core::department::*;
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::id::SessionId;

fn parse_session_id(args: &serde_json::Value) -> Result<SessionId> {
    args.get("session_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .map(SessionId::from_uuid)
        .ok_or_else(|| RusvelError::Validation("session_id required or invalid".into()))
}

pub fn register(engine: &Arc<DistroEngine>, ctx: &mut RegistrationContext) {
    let eng = engine.clone();
    ctx.tools.add(
        "distro",
        "distro.seo.analyze",
        "Summarize tracked SEO keywords (positions, volume) for the session",
        serde_json::json!({
            "type": "object",
            "properties": { "session_id": { "type": "string" } },
            "required": ["session_id"]
        }),
        Arc::new(move |args| {
            let eng = eng.clone();
            Box::pin(async move {
                let sid = parse_session_id(&args)?;
                let keywords = eng.seo().list_keywords(sid).await?;
                let positions: Vec<u32> = keywords.iter().map(|k| k.position).collect();
                let avg_pos = if keywords.is_empty() {
                    0.0
                } else {
                    positions.iter().map(|&p| f64::from(p)).sum::<f64>() / keywords.len() as f64
                };
                let report = serde_json::json!({
                    "keyword_count": keywords.len(),
                    "avg_position": avg_pos,
                    "keywords": keywords,
                });
                Ok(ToolOutput {
                    content: report.to_string(),
                    is_error: false,
                    metadata: report,
                })
            })
        }),
    );

    let eng = engine.clone();
    ctx.tools.add(
        "distro",
        "distro.marketplace.list",
        "List marketplace listings",
        serde_json::json!({
            "type": "object",
            "properties": { "session_id": { "type": "string" } },
            "required": ["session_id"]
        }),
        Arc::new(move |args| {
            let eng = eng.clone();
            Box::pin(async move {
                let sid = parse_session_id(&args)?;
                let list = eng.marketplace().list_listings(sid).await?;
                Ok(ToolOutput {
                    content: serde_json::to_string_pretty(&list).unwrap_or_else(|_| "[]".into()),
                    is_error: false,
                    metadata: serde_json::json!({ "count": list.len() }),
                })
            })
        }),
    );

    let eng = engine.clone();
    ctx.tools.add(
        "distro",
        "distro.affiliate.create_link",
        "Register an affiliate partner (commission-based referral link)",
        serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string" },
                "name": { "type": "string" },
                "commission_rate": { "type": "number", "description": "Rate between 0 and 1" }
            },
            "required": ["session_id", "name", "commission_rate"]
        }),
        Arc::new(move |args| {
            let eng = eng.clone();
            Box::pin(async move {
                let sid = parse_session_id(&args)?;
                let name = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let commission_rate = args
                    .get("commission_rate")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                let p = eng
                    .affiliate()
                    .add_partner(sid, name, commission_rate)
                    .await?;
                Ok(ToolOutput {
                    content: serde_json::json!({ "partner_id": p.id.to_string() }).to_string(),
                    is_error: false,
                    metadata: serde_json::json!({ "partner_id": p.id.to_string() }),
                })
            })
        }),
    );

    let eng = engine.clone();
    ctx.tools.add(
        "distro",
        "distro.analytics.report",
        "Aggregate listings, affiliates, and SEO keywords into one JSON report",
        serde_json::json!({
            "type": "object",
            "properties": { "session_id": { "type": "string" } },
            "required": ["session_id"]
        }),
        Arc::new(move |args| {
            let eng = eng.clone();
            Box::pin(async move {
                let sid = parse_session_id(&args)?;
                let listings = eng.marketplace().list_listings(sid).await?;
                let partners = eng.affiliate().list_partners(sid).await?;
                let keywords = eng.seo().list_keywords(sid).await?;
                let total_revenue: f64 = listings.iter().map(|l| l.revenue).sum();
                let report = serde_json::json!({
                    "listings_count": listings.len(),
                    "partners_count": partners.len(),
                    "keywords_count": keywords.len(),
                    "total_listing_revenue": total_revenue,
                    "listings": listings,
                    "partners": partners,
                    "keywords": keywords,
                });
                Ok(ToolOutput {
                    content: report.to_string(),
                    is_error: false,
                    metadata: report,
                })
            })
        }),
    );

    tracing::debug!("distro agent tools registered");
}
