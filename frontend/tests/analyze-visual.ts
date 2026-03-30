/**
 * Visual Analysis Script — Claude Vision-powered screenshot diff analysis.
 *
 * Reads Playwright test results, finds visual regressions, sends screenshots
 * to Claude Vision for semantic analysis, and outputs a structured report.
 *
 * Usage: node --loader ts-node/esm tests/analyze-visual.ts [--post]
 *   --post  Also POST the report to the RUSVEL API at /api/system/visual-report
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'node:url';
import { execSync } from 'child_process';
import { buildManifest, type ScreenshotManifest } from './utils/screenshot';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const RESULTS_DIR = path.join(__dirname, '..', 'test-results');
const REPORT_FILE = path.join(RESULTS_DIR, 'visual-analysis.json');
const API = 'http://localhost:3000';

interface VisualIssue {
	type: string;
	description: string;
	element: string;
	suggested_fix: string;
}

interface RecommendedAction {
	action_type: 'skill' | 'rule' | 'hook';
	entity_description: string;
}

interface RouteAnalysis {
	route: string;
	severity: 'low' | 'medium' | 'high' | 'critical';
	issues: VisualIssue[];
	recommended_actions: RecommendedAction[];
}

interface VisualReport {
	run_id: string;
	timestamp: string;
	manifest: ScreenshotManifest;
	analyses: RouteAnalysis[];
	summary: {
		total_routes: number;
		regressions: number;
		critical: number;
		high: number;
		medium: number;
		low: number;
	};
}

async function analyzeScreenshot(entry: {
	route: string;
	actualPath: string | null;
	diffPath: string | null;
	baselinePath: string | null;
}): Promise<RouteAnalysis | null> {
	// Need at least the actual and diff images
	if (!entry.actualPath || !entry.diffPath) return null;
	if (!fs.existsSync(entry.actualPath) || !fs.existsSync(entry.diffPath)) return null;

	const actualB64 = fs.readFileSync(entry.actualPath).toString('base64');
	const diffB64 = fs.readFileSync(entry.diffPath).toString('base64');
	const baselineB64 = entry.baselinePath && fs.existsSync(entry.baselinePath)
		? fs.readFileSync(entry.baselinePath).toString('base64')
		: null;

	const prompt = buildAnalysisPrompt(entry.route, baselineB64 !== null);

	// Use claude CLI with vision — pass images as base64 in prompt
	// We write a temp file with the full prompt + image references
	const images: string[] = [];
	if (baselineB64) images.push(`[baseline image for ${entry.route}]: data:image/png;base64,${baselineB64}`);
	images.push(`[current screenshot for ${entry.route}]: data:image/png;base64,${actualB64}`);
	images.push(`[diff highlighting changes for ${entry.route}]: data:image/png;base64,${diffB64}`);

	const fullPrompt = `${prompt}\n\n${images.join('\n\n')}`;

	try {
		// Use claude CLI for analysis (leverages existing RUSVEL infra)
		const tempFile = path.join(RESULTS_DIR, `_prompt_${Date.now()}.txt`);
		fs.writeFileSync(tempFile, fullPrompt);

		const result = execSync(
			`claude -p "$(cat ${tempFile})" --output-format text --model sonnet --no-session-persistence 2>/dev/null`,
			{ maxBuffer: 10 * 1024 * 1024, timeout: 60_000 }
		).toString();

		// Clean up temp file
		fs.unlinkSync(tempFile);

		return parseAnalysisResponse(entry.route, result);
	} catch (err) {
		console.warn(`[analyze] Failed to analyze ${entry.route}:`, (err as Error).message);
		return {
			route: entry.route,
			severity: 'medium',
			issues: [
				{
					type: 'analysis_failed',
					description: 'Could not analyze screenshot diff automatically',
					element: 'unknown',
					suggested_fix: 'Manually inspect the diff images in test-results/'
				}
			],
			recommended_actions: []
		};
	}
}

function buildAnalysisPrompt(route: string, hasBaseline: boolean): string {
	return `You are a UI/UX quality analyzer for RUSVEL, an AI-powered virtual agency app built with SvelteKit 5 + Tailwind CSS 4.

Analyze the visual regression for route "${route}".
${hasBaseline ? 'Compare the baseline (expected) with the current screenshot and the diff image.' : 'Analyze the current screenshot and diff image.'}

Respond ONLY with valid JSON in this exact format:
{
  "severity": "low|medium|high|critical",
  "issues": [
    {
      "type": "layout_broken|style_changed|element_missing|element_added|text_changed|responsive_issue|color_changed",
      "description": "What specifically changed",
      "element": "Component or CSS selector affected",
      "suggested_fix": "How to fix it in the Svelte/Tailwind code"
    }
  ],
  "recommended_actions": [
    {
      "action_type": "skill|rule|hook",
      "entity_description": "Description for !build command to create this entity"
    }
  ]
}

Severity guide:
- critical: Page is broken, elements overlap, content invisible
- high: Significant layout shift, missing components, broken functionality
- medium: Noticeable style changes, color shifts, spacing issues
- low: Minor pixel differences, font rendering, anti-aliasing`;
}

function parseAnalysisResponse(route: string, response: string): RouteAnalysis {
	try {
		// Extract JSON from response (might be wrapped in markdown code blocks)
		const jsonMatch = response.match(/\{[\s\S]*\}/);
		if (!jsonMatch) throw new Error('No JSON found in response');

		const parsed = JSON.parse(jsonMatch[0]);
		return {
			route,
			severity: parsed.severity ?? 'medium',
			issues: parsed.issues ?? [],
			recommended_actions: parsed.recommended_actions ?? []
		};
	} catch {
		return {
			route,
			severity: 'medium',
			issues: [
				{
					type: 'parse_failed',
					description: `Could not parse analysis response: ${response.substring(0, 200)}`,
					element: 'unknown',
					suggested_fix: 'Check the raw analysis output'
				}
			],
			recommended_actions: []
		};
	}
}

async function main() {
	console.log('[analyze-visual] Building screenshot manifest...');
	const manifest = buildManifest(RESULTS_DIR);

	if (manifest.summary.failed === 0) {
		console.log('[analyze-visual] No visual regressions detected. All clear!');
		const report: VisualReport = {
			run_id: manifest.runId,
			timestamp: new Date().toISOString(),
			manifest,
			analyses: [],
			summary: {
				total_routes: manifest.summary.total,
				regressions: 0,
				critical: 0,
				high: 0,
				medium: 0,
				low: 0
			}
		};
		fs.writeFileSync(REPORT_FILE, JSON.stringify(report, null, 2));
		return;
	}

	console.log(
		`[analyze-visual] Found ${manifest.summary.failed} regressions. Analyzing with Claude Vision...`
	);

	const failedEntries = manifest.entries.filter((e) => e.status === 'fail');
	const analyses: RouteAnalysis[] = [];

	for (const entry of failedEntries) {
		console.log(`  Analyzing ${entry.route}...`);
		const analysis = await analyzeScreenshot(entry);
		if (analysis) analyses.push(analysis);
	}

	const report: VisualReport = {
		run_id: manifest.runId,
		timestamp: new Date().toISOString(),
		manifest,
		analyses,
		summary: {
			total_routes: manifest.summary.total,
			regressions: analyses.length,
			critical: analyses.filter((a) => a.severity === 'critical').length,
			high: analyses.filter((a) => a.severity === 'high').length,
			medium: analyses.filter((a) => a.severity === 'medium').length,
			low: analyses.filter((a) => a.severity === 'low').length
		}
	};

	fs.writeFileSync(REPORT_FILE, JSON.stringify(report, null, 2));
	console.log(`[analyze-visual] Report written to ${REPORT_FILE}`);
	console.log(
		`  Summary: ${report.summary.regressions} regressions ` +
			`(${report.summary.critical} critical, ${report.summary.high} high, ` +
			`${report.summary.medium} medium, ${report.summary.low} low)`
	);

	// Optionally POST to RUSVEL API
	if (process.argv.includes('--post')) {
		try {
			const res = await fetch(`${API}/api/system/visual-report`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify(report)
			});
			if (res.ok) {
				console.log('[analyze-visual] Report posted to RUSVEL API');
			} else {
				console.warn(`[analyze-visual] API responded with ${res.status}`);
			}
		} catch (err) {
			console.warn('[analyze-visual] Could not post to API:', (err as Error).message);
		}
	}
}

main().catch(console.error);
