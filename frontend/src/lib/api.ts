// In dev mode (vite), API is on :3000. In production, same origin.
const BASE = import.meta.env.DEV ? 'http://localhost:3000' : '';

import { normalizeDepartmentList, type DepartmentDef } from './departmentManifest';

export type { QuickAction, DepartmentDef } from './departmentManifest';
export {
	normalizeDepartmentList,
	tabsFromDepartment,
	deptHref,
	resolveDeptId,
	deptShellNavItems,
	isDeptShellTabVisible
} from './departmentManifest';

/** Merged into all API fetches when set (dev / custom builds). Token is embedded in static assets — use read-only token when possible. */
function withAuthHeaders(extra?: HeadersInit): Headers {
	const h = new Headers(extra);
	if (!h.has('Content-Type')) {
		h.set('Content-Type', 'application/json');
	}
	const token = import.meta.env.VITE_RUSVEL_API_TOKEN as string | undefined;
	if (token?.trim()) {
		h.set('Authorization', `Bearer ${token.trim()}`);
	}
	return h;
}

export interface SessionSummary {
	id: string;
	name: string;
	kind: string;
	tags: string[];
	updated_at: string;
}

export interface Session {
	id: string;
	name: string;
	kind: string;
	tags: string[];
	config: Record<string, unknown>;
	created_at: string;
	updated_at: string;
	metadata: Record<string, unknown>;
}

export interface Goal {
	id: string;
	session_id: string;
	title: string;
	description: string;
	timeframe: string;
	status: string;
	progress: number;
	metadata: Record<string, unknown>;
}

export interface Task {
	id: string;
	session_id: string;
	goal_id: string | null;
	title: string;
	status: string;
	due_at: string | null;
	priority: string;
	metadata: Record<string, unknown>;
}

export interface DailyPlan {
	date: string;
	tasks: Task[];
	focus_areas: string[];
	notes: string;
	metadata: Record<string, unknown>;
}

export interface Event {
	id: string;
	session_id: string | null;
	run_id: string | null;
	source: string;
	kind: string;
	payload: unknown;
	created_at: string;
	metadata: Record<string, unknown>;
}

// Shared SSE stream parser — used by all streaming endpoints
async function parseSSE(
	res: Response,
	onChunk: (parsed: Record<string, unknown>) => void,
	onError: (message: string) => void
): Promise<void> {
	if (!res.ok) {
		onError(`API error ${res.status}`);
		return;
	}
	const reader = res.body?.getReader();
	if (!reader) {
		onError('No response body');
		return;
	}
	const decoder = new TextDecoder();
	let buffer = '';
	while (true) {
		const { done, value } = await reader.read();
		if (done) break;
		buffer += decoder.decode(value, { stream: true });
		const lines = buffer.split('\n');
		buffer = lines.pop() ?? '';
		for (const line of lines) {
			if (line.startsWith('event: ')) continue;
			if (line.startsWith('data: ')) {
				try {
					onChunk(JSON.parse(line.slice(6)));
				} catch {
					/* skip unparseable */
				}
			}
		}
	}
}

async function request<T>(path: string, options?: RequestInit): Promise<T> {
	const { headers: ho, ...rest } = options ?? {};
	const res = await fetch(`${BASE}${path}`, {
		...rest,
		headers: withAuthHeaders(ho)
	});
	if (!res.ok) {
		const text = await res.text();
		throw new Error(`API error ${res.status}: ${text}`);
	}
	return res.json();
}

export async function checkHealth(): Promise<{ status: string }> {
	return request('/api/health');
}

export async function getSessions(): Promise<SessionSummary[]> {
	return request('/api/sessions');
}

export async function createSession(name: string, kind: string): Promise<{ id: string }> {
	return request('/api/sessions', {
		method: 'POST',
		body: JSON.stringify({ name, kind })
	});
}

export async function getSession(id: string): Promise<Session> {
	return request(`/api/sessions/${id}`);
}

export async function getMissionToday(sessionId: string): Promise<DailyPlan> {
	return request(`/api/sessions/${sessionId}/mission/today`);
}

export async function getGoals(sessionId: string): Promise<Goal[]> {
	return request(`/api/sessions/${sessionId}/mission/goals`);
}

export async function createGoal(
	sessionId: string,
	goal: { title: string; description: string; timeframe: string }
): Promise<Goal> {
	return request(`/api/sessions/${sessionId}/mission/goals`, {
		method: 'POST',
		body: JSON.stringify(goal)
	});
}

export async function getEvents(sessionId: string): Promise<Event[]> {
	return request(`/api/sessions/${sessionId}/events`);
}

/** Last persisted executive brief (`GET /api/brief/latest`); null if none. */
export interface ExecutiveBriefRow {
	id: string;
	date: string;
	summary: string;
	action_items: string[];
	created_at: string;
	sections: unknown[];
}

export async function getBriefLatest(sessionId: string): Promise<ExecutiveBriefRow | null> {
	const res = await fetch(
		`${BASE}/api/brief/latest?session_id=${encodeURIComponent(sessionId)}`,
		{ headers: withAuthHeaders() }
	);
	if (res.status === 404) return null;
	if (!res.ok) {
		const text = await res.text();
		throw new Error(`API error ${res.status}: ${text}`);
	}
	return res.json() as Promise<ExecutiveBriefRow>;
}

// ── Config (M02, M03, M04) ───────────────────────────────────

export interface ChatConfig {
	model: string;
	effort: string;
	max_budget_usd: number | null;
	permission_mode: string;
	allowed_tools: string[];
	disallowed_tools: string[];
	max_turns: number | null;
	/** Optional tier hint from API (`fast` | `balanced` | `premium`). */
	model_tier?: string | null;
}

export interface ModelOption {
	value: string;
	label: string;
	description: string;
}

export interface ToolOption {
	name: string;
	description: string;
	category: string;
}

export async function getConfig(): Promise<ChatConfig> {
	return request('/api/config');
}

export async function updateConfig(config: ChatConfig): Promise<ChatConfig> {
	return request('/api/config', {
		method: 'PUT',
		body: JSON.stringify(config)
	});
}

export async function getModels(): Promise<ModelOption[]> {
	return request('/api/config/models');
}

/** Live wiring + cheap health probes (CLI `--version`, Ollama /api/tags). No secrets. */
export interface LlmProviderStatusRow {
	id: string;
	display_name: string;
	wired: boolean;
	route: string;
	healthy: boolean | null;
	detail: string | null;
}

export interface LlmProvidersReport {
	providers: LlmProviderStatusRow[];
	ollama_host: string;
	ollama_models: string[];
	/** Running server: `claude/…` goes to Messages API or `claude -p`. */
	claude_effective_transport: 'api' | 'cli';
	/** True when RUSVEL_USE_CLAUDE_CLI overrode a present API key. */
	claude_cli_forced_by_env: boolean;
	/** True when operator_prefs.force_claude_cli was true at boot. */
	claude_cli_forced_by_operator_pref: boolean;
}

export interface OperatorCapabilityRow {
	id: string;
	label: string;
	settings_href: string | null;
	doc_path: string;
}

export interface SystemRuntimeResponse {
	process: { uptime_seconds: number; data_dir: string; http_listen: string };
	auth: { admin_token_configured: boolean; read_token_configured: boolean };
	llm: LlmProvidersReport;
	drift: {
		claude: {
			env_suggests_cli: boolean;
			effective_cli: boolean;
			operator_pref_force_claude_cli: boolean | null;
		};
	};
	integrations: {
		telegram_channel_configured: boolean;
		smtp_host_set: boolean;
		mcp_http_auth_env: boolean;
	};
	subsystems: Record<string, boolean>;
	health: { database: string; departments_failed: unknown[] };
	operator_prefs_stored: {
		force_claude_cli: boolean | null;
		metadata: unknown;
	};
	capabilities: OperatorCapabilityRow[];
	notes: Record<string, string>;
}

export async function getSystemRuntime(): Promise<SystemRuntimeResponse> {
	return request('/api/system/runtime');
}

export async function updateOperatorPrefs(body: {
	force_claude_cli: boolean | null;
}): Promise<{ requires_restart: boolean; message: string; saved: unknown }> {
	return request('/api/system/operator-prefs', {
		method: 'PUT',
		body: JSON.stringify(body)
	});
}

export async function postShutdown(): Promise<{ ok: boolean; message: string }> {
	return request('/api/system/shutdown', { method: 'POST', body: '{}' });
}

export async function getLlmProviders(): Promise<LlmProvidersReport> {
	return request('/api/config/llm-providers');
}

export async function getTools(): Promise<ToolOption[]> {
	return request('/api/config/tools');
}

// ── Department API (shared pattern for Code/Content/Harvest/GTM) ──

export interface ContextPackFlags {
	session_name?: boolean | null;
	goals?: boolean | null;
	events?: boolean | null;
	metrics?: boolean | null;
}

export interface DepartmentConfig {
	engine: string;
	model: string;
	effort: string;
	max_budget_usd: number | null;
	permission_mode: string;
	allowed_tools: string[];
	disallowed_tools: string[];
	system_prompt: string;
	add_dirs: string[];
	max_turns: number | null;
	/** S-045: optional section toggles for session context pack */
	context_pack?: ContextPackFlags | null;
	/** `discuss` vs `agentic` — dept chat system prompt bias */
	chat_mode?: string;
}

export async function getDeptConfig(dept: string): Promise<DepartmentConfig> {
	return request(`/api/dept/${dept}/config`);
}

export async function updateDeptConfig(
	dept: string,
	config: DepartmentConfig
): Promise<DepartmentConfig> {
	return request(`/api/dept/${dept}/config`, { method: 'PUT', body: JSON.stringify(config) });
}

export async function getDeptConversations(dept: string): Promise<Conversation[]> {
	return request(`/api/dept/${dept}/chat/conversations`);
}

export async function getDeptChatHistory(dept: string, id: string): Promise<ChatMessage[]> {
	return request(`/api/dept/${dept}/chat/conversations/${id}`);
}

export async function getDeptEvents(dept: string): Promise<Event[]> {
	return request(`/api/dept/${dept}/events`);
}

export interface AnalyticsSpendResponse {
	total_usd: number;
	by_department: Record<string, number>;
	session_id?: string;
	session_total_usd?: number;
	session_budget_limit_usd?: number;
	budget_warning: boolean;
	budget_usage_ratio?: number;
	/** Token totals per department when spend is sourced from cost_events */
	department_tokens?: Record<string, number>;
	/** Per-model USD + tokens when cost_events provide model breakdown */
	by_model?: Record<string, { usd: number; tokens: number }>;
}

/** GET /api/analytics/spend — LLM spend by department; optional session for budget context (S-035). */
export async function getAnalyticsSpend(
	dept?: string,
	sessionId?: string | null
): Promise<AnalyticsSpendResponse> {
	const sp = new URLSearchParams();
	if (dept) sp.set('dept', dept);
	if (sessionId) sp.set('session_id', sessionId);
	const q = sp.toString();
	return request<AnalyticsSpendResponse>(`/api/analytics/spend${q ? `?${q}` : ''}`);
}

export interface CostSummaryRow {
	key: string;
	total_usd: number;
	total_tokens: number;
}

/** GET /api/analytics/costs/summary — aggregated cost rows for dashboard tables. */
export async function getCostSummaryByGroup(
	groupBy: 'department' | 'model' | 'operation'
): Promise<CostSummaryRow[]> {
	const sp = new URLSearchParams({ group_by: groupBy });
	return request<CostSummaryRow[]>(`/api/analytics/costs/summary?${sp}`);
}

export async function streamDeptChat(
	dept: string,
	message: string,
	conversationId: string | undefined,
	sessionId: string | null | undefined,
	onDelta: (text: string, conversationId: string) => void,
	onDone: (fullText: string, conversationId: string) => void,
	onError: (message: string) => void,
	onToolCall?: (id: string, name: string, args: Record<string, unknown>, conversationId: string) => void,
	onToolResult?: (id: string, name: string, result: string, isError: boolean, conversationId: string) => void
): Promise<void> {
	const res = await fetch(`${BASE}/api/dept/${dept}/chat`, {
		method: 'POST',
		headers: withAuthHeaders(),
		body: JSON.stringify({
			message,
			conversation_id: conversationId,
			...(sessionId ? { session_id: sessionId } : {})
		})
	});
	await parseSSE(
		res,
		(p) => {
			const t = p.type as string | undefined;
			if (t === 'tool_call_start' && onToolCall) {
				onToolCall(
					p.tool_call_id as string,
					p.tool_name as string,
					p.args as Record<string, unknown>,
					p.conversation_id as string
				);
			} else if (t === 'tool_call_end' && onToolResult) {
				onToolResult(
					p.tool_call_id as string,
					p.tool_name as string,
					p.output as string,
					(p.is_error as boolean) ?? false,
					p.conversation_id as string
				);
			} else if (
				t === 'text_delta' ||
				(p.text !== undefined && p.cost_usd === undefined && t !== 'run_completed' && t !== 'run_failed')
			) {
				onDelta(p.text as string, p.conversation_id as string);
			} else if (t === 'run_completed' || (p.cost_usd !== undefined && p.text !== undefined)) {
				const full =
					t === 'run_completed' ? (p.output as string) : (p.text as string);
				onDone(full, p.conversation_id as string);
			} else if (t === 'run_failed') {
				onError((p.error as string) ?? 'run failed');
			} else if (p.message) onError(p.message as string);
		},
		onError
	);
}

// ── Agents CRUD ──────────────────────────────────────────────

export interface Agent {
	id: string;
	name: string;
	role: string;
	instructions: string;
	default_model: { provider: string; model: string };
	allowed_tools: string[];
	capabilities: string[];
	budget_limit: number | null;
	metadata: Record<string, unknown>;
}

export async function getAgents(engine?: string): Promise<Agent[]> {
	return request(`/api/agents${engine ? `?engine=${engine}` : ''}`);
}

export async function createAgent(agent: {
	name: string;
	role?: string;
	instructions?: string;
	model?: string;
	allowed_tools?: string[];
	budget_limit?: number;
	metadata?: Record<string, unknown>;
}): Promise<Agent> {
	return request('/api/agents', { method: 'POST', body: JSON.stringify(agent) });
}

export async function deleteAgent(id: string): Promise<void> {
	await fetch(`${BASE}/api/agents/${id}`, { method: 'DELETE', headers: withAuthHeaders() });
}

// ── Skills CRUD ──────────────────────────────────────────────

export interface Skill {
	id: string;
	name: string;
	description: string;
	prompt_template: string;
	metadata: Record<string, unknown>;
}

export async function getSkills(engine?: string): Promise<Skill[]> {
	return request(`/api/skills${engine ? `?engine=${engine}` : ''}`);
}

export async function createSkill(skill: Partial<Skill>): Promise<Skill> {
	return request('/api/skills', { method: 'POST', body: JSON.stringify(skill) });
}

export async function deleteSkill(id: string): Promise<void> {
	await fetch(`${BASE}/api/skills/${id}`, { method: 'DELETE', headers: withAuthHeaders() });
}

// ── Rules CRUD ───────────────────────────────────────────────

export interface Rule {
	id: string;
	name: string;
	content: string;
	enabled: boolean;
	metadata: Record<string, unknown>;
}

export async function getRules(engine?: string): Promise<Rule[]> {
	return request(`/api/rules${engine ? `?engine=${engine}` : ''}`);
}

export async function createRule(rule: Partial<Rule>): Promise<Rule> {
	return request('/api/rules', { method: 'POST', body: JSON.stringify(rule) });
}

export async function updateRule(id: string, rule: Partial<Rule>): Promise<Rule> {
	return request(`/api/rules/${id}`, { method: 'PUT', body: JSON.stringify(rule) });
}

export async function deleteRule(id: string): Promise<void> {
	await fetch(`${BASE}/api/rules/${id}`, { method: 'DELETE', headers: withAuthHeaders() });
}

// ── MCP Servers CRUD ─────────────────────────────────────────

export interface McpServer {
	id: string;
	name: string;
	description: string;
	server_type: string;
	command: string | null;
	args: string[];
	url: string | null;
	env: Record<string, unknown>;
	enabled: boolean;
	metadata: Record<string, unknown>;
}

export async function getMcpServers(engine?: string): Promise<McpServer[]> {
	return request(`/api/mcp-servers${engine ? `?engine=${engine}` : ''}`);
}

export async function createMcpServer(server: Partial<McpServer>): Promise<McpServer> {
	return request('/api/mcp-servers', { method: 'POST', body: JSON.stringify(server) });
}

export async function deleteMcpServer(id: string): Promise<void> {
	await fetch(`${BASE}/api/mcp-servers/${id}`, { method: 'DELETE', headers: withAuthHeaders() });
}

// ── Hooks CRUD ───────────────────────────────────────────────

export interface Hook {
	id: string;
	name: string;
	event: string;
	matcher: string;
	hook_type: string;
	action: string;
	enabled: boolean;
	metadata: Record<string, unknown>;
}

export async function getHooks(engine?: string): Promise<Hook[]> {
	return request(`/api/hooks${engine ? `?engine=${engine}` : ''}`);
}

export async function createHook(hook: Partial<Hook>): Promise<Hook> {
	return request('/api/hooks', { method: 'POST', body: JSON.stringify(hook) });
}

export async function updateHook(id: string, hook: Partial<Hook>): Promise<Hook> {
	return request(`/api/hooks/${id}`, { method: 'PUT', body: JSON.stringify(hook) });
}

export async function deleteHook(id: string): Promise<void> {
	await fetch(`${BASE}/api/hooks/${id}`, { method: 'DELETE', headers: withAuthHeaders() });
}

export async function getHookEvents(): Promise<string[]> {
	return request('/api/hooks/events');
}

// ── Workflows CRUD + Execution ────────────────────────────────

export interface WorkflowStepDef {
	agent_name: string;
	prompt_template: string;
	step_type: string;
}

export interface Workflow {
	id: string;
	name: string;
	description: string;
	steps: WorkflowStepDef[];
	metadata: Record<string, unknown>;
}

export interface StepResult {
	step_index: number;
	agent_name: string;
	prompt: string;
	output: string;
	cost_usd: number;
}

export interface WorkflowRunResult {
	workflow_id: string;
	workflow_name: string;
	steps: StepResult[];
	total_cost_usd: number;
}

export async function getWorkflows(): Promise<Workflow[]> {
	return request('/api/workflows');
}

export async function createWorkflow(workflow: {
	name: string;
	description?: string;
	steps: WorkflowStepDef[];
	metadata?: Record<string, unknown>;
}): Promise<Workflow> {
	return request('/api/workflows', { method: 'POST', body: JSON.stringify(workflow) });
}

export async function updateWorkflow(id: string, workflow: Workflow): Promise<Workflow> {
	return request(`/api/workflows/${id}`, { method: 'PUT', body: JSON.stringify(workflow) });
}

export async function deleteWorkflow(id: string): Promise<void> {
	await fetch(`${BASE}/api/workflows/${id}`, { method: 'DELETE', headers: withAuthHeaders() });
}

export async function runWorkflow(
	id: string,
	variables?: Record<string, string>
): Promise<WorkflowRunResult> {
	return request(`/api/workflows/${id}/run`, {
		method: 'POST',
		body: JSON.stringify({ variables: variables ?? {} })
	});
}

// ── Approvals (Human-in-the-loop, ADR-008) ───────────────────

export interface Job {
	id: string;
	session_id: string;
	/** Serialized `JobKind`; may be a string or `{"Custom":"..."}` from the API. */
	kind: string | Record<string, unknown>;
	payload: unknown;
	status: string;
	scheduled_at: string | null;
	started_at: string | null;
	completed_at: string | null;
	retries: number;
	max_retries: number;
	error: string | null;
	metadata: Record<string, unknown>;
	/** Set on some approval jobs for deep-linking to the department terminal. */
	terminal_pane_id?: string;
	terminal_window_id?: string;
	terminal_dept_id?: string;
}

export async function getPendingApprovals(): Promise<Job[]> {
	return request('/api/approvals');
}

async function postApprovalAction(path: string, body?: Record<string, unknown>): Promise<void> {
	const res = await fetch(`${BASE}${path}`, {
		method: 'POST',
		headers: withAuthHeaders(),
		body: body !== undefined ? JSON.stringify(body) : undefined
	});
	if (!res.ok) {
		const text = await res.text();
		throw new Error(`API error ${res.status}: ${text}`);
	}
}

export async function approveJob(id: string): Promise<void> {
	return postApprovalAction(`/api/approvals/${id}/approve`, {});
}

export async function rejectJob(id: string, reason?: string): Promise<void> {
	return postApprovalAction(`/api/approvals/${id}/reject`, {
		...(reason?.trim() ? { reason: reason.trim() } : {})
	});
}

// ── Chat (God Agent) ─────────────────────────────────────────

export interface ChatMessage {
	id: string;
	conversation_id: string;
	role: 'user' | 'assistant' | 'system';
	content: string;
	created_at: string;
}

export interface Conversation {
	id: string;
	title: string;
	updated_at: string;
	message_count: number;
}

export async function getConversations(): Promise<Conversation[]> {
	return request('/api/chat/conversations');
}

export async function getChatHistory(conversationId: string): Promise<ChatMessage[]> {
	return request(`/api/chat/conversations/${conversationId}`);
}

// ── Capability Engine ────────────────────────────────────────

export async function streamCapability(
	description: string,
	engine: string | undefined,
	onDelta: (text: string) => void,
	onDone: (fullText: string) => void,
	onError: (message: string) => void
): Promise<void> {
	const res = await fetch(`${BASE}/api/capability/build`, {
		method: 'POST',
		headers: withAuthHeaders(),
		body: JSON.stringify({ description, engine })
	});
	await parseSSE(
		res,
		(p) => {
			if (p.text !== undefined && p.cost_usd === undefined) onDelta(p.text as string);
			else if (p.cost_usd !== undefined) onDone(p.text as string);
			else if (p.message) onError(p.message as string);
		},
		onError
	);
}

// ── Help (AI-powered) ────────────────────────────────────────

export async function streamHelp(
	question: string,
	context: string | undefined,
	onDelta: (text: string) => void,
	onDone: (fullText: string) => void,
	onError: (message: string) => void
): Promise<void> {
	const res = await fetch(`${BASE}/api/help`, {
		method: 'POST',
		headers: withAuthHeaders(),
		body: JSON.stringify({ question, context })
	});
	await parseSSE(
		res,
		(p) => {
			if (p.text !== undefined && p.cost_usd === undefined) onDelta(p.text as string);
			else if (p.cost_usd !== undefined) onDone(p.text as string);
			else if (p.message) onError(p.message as string);
		},
		onError
	);
}

// ── Chat (God Agent) ─────────────────────────────────────────

export async function streamChat(
	message: string,
	conversationId: string | undefined,
	sessionId: string | null | undefined,
	onDelta: (text: string, conversationId: string) => void,
	onDone: (fullText: string, conversationId: string) => void,
	onError: (message: string) => void,
	onToolCall?: (id: string, name: string, args: Record<string, unknown>, conversationId: string) => void,
	onToolResult?: (id: string, name: string, result: string, isError: boolean, conversationId: string) => void
): Promise<void> {
	const res = await fetch(`${BASE}/api/chat`, {
		method: 'POST',
		headers: withAuthHeaders(),
		body: JSON.stringify({
			message,
			conversation_id: conversationId,
			...(sessionId ? { session_id: sessionId } : {})
		})
	});
	await parseSSE(
		res,
		(p) => {
			const t = p.type as string | undefined;
			if (t === 'tool_call_start' && onToolCall) {
				onToolCall(
					p.tool_call_id as string,
					p.tool_name as string,
					p.args as Record<string, unknown>,
					p.conversation_id as string
				);
			} else if (t === 'tool_call_end' && onToolResult) {
				onToolResult(
					p.tool_call_id as string,
					p.tool_name as string,
					p.output as string,
					(p.is_error as boolean) ?? false,
					p.conversation_id as string
				);
			} else if (
				t === 'text_delta' ||
				(p.text !== undefined && p.cost_usd === undefined && t !== 'run_completed' && t !== 'run_failed')
			) {
				onDelta(p.text as string, p.conversation_id as string);
			} else if (t === 'run_completed' || (p.cost_usd !== undefined && p.text !== undefined)) {
				const full =
					t === 'run_completed' ? (p.output as string) : (p.text as string);
				onDone(full, p.conversation_id as string);
			} else if (t === 'run_failed') {
				onError((p.error as string) ?? 'run failed');
			} else if (p.message) onError(p.message as string);
		},
		onError
	);
}

// ── Department Registry ──────────────────────────────────────

export async function getDepartments(): Promise<DepartmentDef[]> {
	const res = await fetch(`${BASE}/api/departments`, { headers: withAuthHeaders() });
	const raw: unknown = await res.json();
	return normalizeDepartmentList(raw);
}

export interface DbTableSummary {
	name: string;
	row_count: number;
}
export interface DbColumnInfo {
	name: string;
	col_type: string;
	nullable: boolean;
	default_value: string | null;
	primary_key: boolean;
}
export interface DbIndexInfo {
	name: string;
	columns: string[];
	unique: boolean;
}
export interface DbForeignKeyInfo {
	from_column: string;
	to_table: string;
	to_column: string;
}
export interface DbTableInfo {
	name: string;
	columns: DbColumnInfo[];
	indexes: DbIndexInfo[];
	foreign_keys: DbForeignKeyInfo[];
	row_count: number;
}
export interface DbSqlColumnMeta {
	name: string;
	type: string;
}
export interface DbRowsResponse {
	columns: DbSqlColumnMeta[];
	rows: unknown[][];
	row_count: number;
	table_row_count: number;
}
export interface DbSqlExecuteResponse {
	columns: DbSqlColumnMeta[];
	rows: unknown[][];
	row_count: number;
	duration_ms: number;
}
export async function getDbTables(): Promise<DbTableSummary[]> {
	const res = await fetch(`${BASE}/api/db/tables`, { headers: withAuthHeaders() });
	if (!res.ok) throw new Error(await res.text());
	return res.json();
}
export async function getDbTableSchema(table: string): Promise<DbTableInfo> {
	const res = await fetch(
		`${BASE}/api/db/tables/${encodeURIComponent(table)}/schema`,
		{ headers: withAuthHeaders() }
	);
	if (!res.ok) throw new Error(await res.text());
	return res.json();
}
export async function getDbTableRows(
	table: string,
	params?: { limit?: number; offset?: number; order?: string }
): Promise<DbRowsResponse> {
	const sp = new URLSearchParams();
	if (params?.limit != null) sp.set('limit', String(params.limit));
	if (params?.offset != null) sp.set('offset', String(params.offset));
	if (params?.order) sp.set('order', params.order);
	const q = sp.toString();
	const url = `${BASE}/api/db/tables/${encodeURIComponent(table)}/rows${q ? `?${q}` : ''}`;
	const res = await fetch(url, { headers: withAuthHeaders() });
	if (!res.ok) throw new Error(await res.text());
	return res.json();
}
export async function postDbSql(
	query: string,
	readOnly = true
): Promise<DbSqlExecuteResponse> {
	return request('/api/db/sql', {
		method: 'POST',
		body: JSON.stringify({ query, read_only: readOnly })
	});
}
export async function postCodeAnalyze(path: string): Promise<unknown> {
	return request('/api/dept/code/analyze', {
		method: 'POST',
		body: JSON.stringify({ path })
	});
}
export async function getCodeSearch(q: string, limit?: number): Promise<unknown> {
	const sp = new URLSearchParams({ q });
	if (limit != null) sp.set('limit', String(limit));
	return request(`/api/dept/code/search?${sp}`);
}
export async function postContentDraft(
	sessionId: string,
	topic: string,
	kind?: string
): Promise<unknown> {
	return request('/api/dept/content/draft', {
		method: 'POST',
		body: JSON.stringify({
			session_id: sessionId,
			topic,
			...(kind ? { kind } : {})
		})
	});
}
/** Shape of `ContentItem` from the content engine list API. */
export interface ContentItemRow {
	id: string;
	title: string;
	body_markdown: string;
	kind: string;
	status: string;
	approval?: string;
	scheduled_at?: string | null;
	published_at?: string | null;
	metadata?: Record<string, unknown>;
}

export async function getContentList(sessionId: string): Promise<ContentItemRow[]> {
	const sp = new URLSearchParams({ session_id: sessionId });
	return request<ContentItemRow[]>(`/api/dept/content/list?${sp}`);
}
export async function getHarvestPipeline(sessionId: string): Promise<unknown> {
	const sp = new URLSearchParams({ session_id: sessionId });
	return request(`/api/dept/harvest/pipeline?${sp}`);
}

/** `POST /api/dept/harvest/scan` — sources: mock | upwork | freelancer | cdp; `query` = RSS terms or CDP listing URL. */
export interface HarvestScanRequest {
	session_id: string;
	sources: string[];
	query: string;
	cdp_extract_js?: string | null;
	cdp_endpoint?: string | null;
}

export async function postHarvestScan(body: HarvestScanRequest): Promise<OpportunityRow[]> {
	const payload: Record<string, unknown> = {
		session_id: body.session_id,
		sources: body.sources,
		query: body.query
	};
	const js = body.cdp_extract_js?.trim();
	if (js) payload.cdp_extract_js = js;
	const ep = body.cdp_endpoint?.trim();
	if (ep) payload.cdp_endpoint = ep;
	return request<OpportunityRow[]>('/api/dept/harvest/scan', {
		method: 'POST',
		body: JSON.stringify(payload)
	});
}

export async function getBrowserStatus(): Promise<{ connected: boolean }> {
	return request('/api/browser/status');
}

export async function postBrowserConnect(
	endpoint: string
): Promise<{ ok: boolean; endpoint: string }> {
	return request('/api/browser/connect', {
		method: 'POST',
		body: JSON.stringify({ endpoint: endpoint.trim() })
	});
}

export async function getBrowserTabs(): Promise<unknown> {
	return request('/api/browser/tabs');
}

export async function postBrowserObserve(
	tabId: string,
	opts?: { session_id?: string; platform?: string }
): Promise<{ ok: boolean; tab_id: string; terminal_pane_id?: string }> {
	const sp = new URLSearchParams();
	if (opts?.session_id) sp.set('session_id', opts.session_id);
	if (opts?.platform) sp.set('platform', opts.platform);
	const q = sp.toString();
	return request(
		`/api/browser/observe/${encodeURIComponent(tabId)}${q ? `?${q}` : ''}`,
		{ method: 'POST' }
	);
}

export async function postTerminalDeptTrace(
	deptId: string,
	sessionId: string,
	message: string
): Promise<void> {
	const sp = new URLSearchParams({ session_id: sessionId });
	await request(`/api/terminal/dept/${encodeURIComponent(deptId)}/trace?${sp}`, {
		method: 'POST',
		body: JSON.stringify({ message })
	});
}

/** `POST /api/forge/pipeline` — cross-engine harvest → content pipeline (S-042). */
export interface ForgePipelineDef {
	steps?: ('scan' | 'score' | 'propose' | 'draft_content')[];
	proposal_profile?: string;
	draft_topic?: string;
	draft_kind?: string;
}

export interface FlowExecutionResponse {
	id: string;
	flow_id: string;
	status: string;
	trigger_data: Record<string, unknown>;
	node_results: Record<string, unknown>;
	started_at: string;
	finished_at: string | null;
	error: string | null;
	metadata: Record<string, unknown>;
}

export async function postForgePipeline(
	sessionId: string,
	def?: ForgePipelineDef
): Promise<FlowExecutionResponse> {
	return request<FlowExecutionResponse>('/api/forge/pipeline', {
		method: 'POST',
		body: JSON.stringify({
			session_id: sessionId,
			...(def && Object.keys(def).length > 0 ? { def } : {})
		})
	});
}

/** CRM contact from `GET /api/dept/gtm/contacts` (S-036). */
export interface GtmContactRow {
	id: string;
	session_id: string;
	name: string;
	emails: string[];
	links: string[];
	company: string | null;
	role: string | null;
	tags: string[];
	last_contacted_at: string | null;
	metadata: Record<string, unknown>;
}

export async function getGtmContacts(sessionId: string): Promise<GtmContactRow[]> {
	const sp = new URLSearchParams({ session_id: sessionId });
	return request(`/api/dept/gtm/contacts?${sp}`);
}

export async function postGtmContact(
	sessionId: string,
	body: {
		name: string;
		email: string;
		company?: string | null;
		role?: string | null;
		tags?: string[];
		links?: string[];
	}
): Promise<{ id: string }> {
	return request('/api/dept/gtm/contacts', {
		method: 'POST',
		body: JSON.stringify({
			session_id: sessionId,
			name: body.name,
			email: body.email,
			company: body.company ?? null,
			role: body.role ?? null,
			tags: body.tags ?? [],
			links: body.links ?? []
		})
	});
}

export type GtmDealStage =
	| 'Lead'
	| 'Qualified'
	| 'Proposal'
	| 'Negotiation'
	| 'Won'
	| 'Lost';

export interface GtmDealRow {
	id: string;
	contact_id: string;
	title: string;
	value: number;
	stage: GtmDealStage;
	notes: string;
	created_at: string;
	contact_name: string | null;
	last_activity: string;
}

export async function getGtmDeals(sessionId: string, stage?: string): Promise<GtmDealRow[]> {
	const sp = new URLSearchParams({ session_id: sessionId });
	if (stage) sp.set('stage', stage);
	return request(`/api/dept/gtm/deals?${sp}`);
}

export async function postGtmDealAdvance(
	sessionId: string,
	dealId: string,
	stage: GtmDealStage
): Promise<void> {
	await request('/api/dept/gtm/deals/advance', {
		method: 'POST',
		body: JSON.stringify({
			session_id: sessionId,
			deal_id: dealId,
			stage
		})
	});
}

/** Aligns with `gtm_engine::InvoiceStatus` (S-038). */
export type GtmInvoiceStatus = 'Draft' | 'Sent' | 'Paid' | 'Overdue' | 'Cancelled';

export interface GtmLineItem {
	description: string;
	quantity: number;
	unit_price: number;
}

export interface GtmInvoiceRow {
	id: string;
	contact_id: string;
	contact_name: string | null;
	items: GtmLineItem[];
	total: number;
	status: GtmInvoiceStatus;
	due_date: string;
	paid_at: string | null;
	metadata: Record<string, unknown>;
}

export async function getGtmInvoices(
	sessionId: string,
	status?: GtmInvoiceStatus
): Promise<GtmInvoiceRow[]> {
	const sp = new URLSearchParams({ session_id: sessionId });
	if (status) sp.set('status', status);
	return request(`/api/dept/gtm/invoices?${sp}`);
}

export async function postGtmInvoice(body: {
	session_id: string;
	contact_id: string;
	items: GtmLineItem[];
	due_date: string;
}): Promise<{ id: string }> {
	return request('/api/dept/gtm/invoices', {
		method: 'POST',
		body: JSON.stringify(body)
	});
}

export interface GtmInvoiceDetail extends GtmInvoiceRow {
	session_id: string;
}

export async function getGtmInvoice(
	sessionId: string,
	invoiceId: string
): Promise<GtmInvoiceDetail> {
	const sp = new URLSearchParams({ session_id: sessionId });
	return request(`/api/dept/gtm/invoices/${encodeURIComponent(invoiceId)}?${sp}`);
}

export async function postGtmInvoiceStatus(
	sessionId: string,
	invoiceId: string,
	status: GtmInvoiceStatus
): Promise<void> {
	await request(`/api/dept/gtm/invoices/${encodeURIComponent(invoiceId)}/status`, {
		method: 'POST',
		body: JSON.stringify({ session_id: sessionId, status })
	});
}

/** Aligns with `gtm_engine::SequenceStatus` (S-033). */
export type GtmSequenceStatus = 'Draft' | 'Active' | 'Paused' | 'Completed';

export interface GtmSequenceStep {
	delay_days: number;
	channel: string;
	template: string;
}

export interface GtmOutreachSequenceRow {
	id: string;
	session_id: string;
	name: string;
	steps: GtmSequenceStep[];
	status: GtmSequenceStatus;
	created_at: string;
	metadata: Record<string, unknown>;
}

export async function getGtmOutreachSequences(
	sessionId: string
): Promise<GtmOutreachSequenceRow[]> {
	const sp = new URLSearchParams({ session_id: sessionId });
	return request(`/api/dept/gtm/outreach/sequences?${sp}`);
}

export async function postGtmOutreachSequence(
	sessionId: string,
	body: { name: string; steps: GtmSequenceStep[] }
): Promise<{ id: string }> {
	return request('/api/dept/gtm/outreach/sequences', {
		method: 'POST',
		body: JSON.stringify({
			session_id: sessionId,
			name: body.name,
			steps: body.steps
		})
	});
}

export async function postGtmOutreachSequenceActivate(
	sessionId: string,
	sequenceId: string
): Promise<void> {
	await request(
		`/api/dept/gtm/outreach/sequences/${encodeURIComponent(sequenceId)}/activate`,
		{
			method: 'POST',
			body: JSON.stringify({ session_id: sessionId })
		}
	);
}

export async function postGtmOutreachExecute(
	sessionId: string,
	sequenceId: string,
	contactId: string
): Promise<{ job_id: string; job_ids: string[]; count: number }> {
	return request('/api/dept/gtm/outreach/execute', {
		method: 'POST',
		body: JSON.stringify({
			session_id: sessionId,
			sequence_id: sequenceId,
			contact_id: contactId
		})
	});
}

export interface OpportunityRow {
	id: string;
	title: string;
	description: string;
	url?: string | null;
	score: number;
	stage: string;
	value_estimate?: number | null;
	metadata: Record<string, unknown>;
	source?: string | Record<string, unknown>;
}

export async function getHarvestOpportunities(
	sessionId: string,
	stage?: string
): Promise<OpportunityRow[]> {
	const sp = new URLSearchParams({ session_id: sessionId });
	if (stage) sp.set('stage', stage);
	return request(`/api/dept/harvest/list?${sp}`);
}

/** POST /api/dept/harvest/ingest — push job-shaped rows (extension/webhook); same scoring + dedup as scan. */
export async function postHarvestIngest(
	sessionId: string,
	jobs: Record<string, unknown>[],
	opts?: { platform?: string }
): Promise<OpportunityRow[]> {
	const body: Record<string, unknown> = {
		session_id: sessionId,
		jobs
	};
	if (opts?.platform != null && opts.platform !== '') {
		body.platform = opts.platform;
	}
	return request<OpportunityRow[]>('/api/dept/harvest/ingest', {
		method: 'POST',
		body: JSON.stringify(body)
	});
}

export async function postHarvestAdvance(
	sessionId: string,
	opportunityId: string,
	stage: string
): Promise<void> {
	await request('/api/dept/harvest/advance', {
		method: 'POST',
		body: JSON.stringify({
			session_id: sessionId,
			opportunity_id: opportunityId,
			stage
		})
	});
}

/** Omitted or `sync: false` enqueues ProposalDraft; worker + approvals. `sync: true` returns proposal JSON inline. */
export type HarvestProposalResponse =
	| {
			job_id: string;
			status: 'queued';
			message?: string;
	  }
	| Record<string, unknown>;

export async function postHarvestProposal(
	sessionId: string,
	opportunityId: string,
	profile: string,
	opts?: { sync?: boolean }
): Promise<HarvestProposalResponse> {
	const body: Record<string, unknown> = {
		session_id: sessionId,
		opportunity_id: opportunityId,
		profile
	};
	if (opts?.sync === true) {
		body.sync = true;
	}
	return request<HarvestProposalResponse>('/api/dept/harvest/proposal', {
		method: 'POST',
		body: JSON.stringify(body)
	});
}

/** GET /api/jobs — `kind` and `status` are plain strings (matches filter query vocabulary). */
export interface JobListItem {
	id: string;
	session_id: string;
	kind: string;
	status: string;
	payload: Record<string, unknown>;
	scheduled_at?: string | null;
	started_at?: string | null;
	completed_at?: string | null;
	retries: number;
	max_retries: number;
	error?: string | null;
	metadata: Record<string, unknown>;
}

export async function getJobs(
	sessionId: string,
	opts?: {
		statuses?: string[];
		kinds?: string[];
		limit?: number;
	}
): Promise<JobListItem[]> {
	const sp = new URLSearchParams({ session_id: sessionId });
	if (opts?.statuses?.length) {
		sp.set('status', opts.statuses.join(','));
	}
	if (opts?.kinds?.length) {
		sp.set('kinds', opts.kinds.join(','));
	}
	if (opts?.limit != null) {
		sp.set('limit', String(opts.limit));
	}
	return request<JobListItem[]>(`/api/jobs?${sp}`);
}

/** GET /api/dashboard/active — queued/running jobs, pending approvals, cron. */
export interface ActiveDashboardResponse {
	jobs: JobListItem[];
	pending_approvals: Job[];
	cron_schedules: unknown[];
}

export async function getActiveDashboard(
	sessionId?: string | null
): Promise<ActiveDashboardResponse> {
	const sp = new URLSearchParams();
	if (sessionId) sp.set('session_id', sessionId);
	const q = sp.toString();
	return request(`/api/dashboard/active${q ? `?${q}` : ''}`);
}

/** GET/POST /api/artifacts — saved outputs (Claude-style artifacts). */
export interface ArtifactRecord {
	id: string;
	title: string;
	kind?: string;
	body: string;
	session_id?: string | null;
	department?: string | null;
	created_at: string;
	metadata?: Record<string, unknown>;
}

export async function listArtifacts(): Promise<ArtifactRecord[]> {
	return request('/api/artifacts');
}

export async function createArtifact(body: {
	title: string;
	kind?: string;
	body: string;
	session_id?: string | null;
	department?: string | null;
	metadata?: Record<string, unknown>;
}): Promise<ArtifactRecord> {
	return request('/api/artifacts', {
		method: 'POST',
		body: JSON.stringify(body)
	});
}

export async function deleteArtifact(id: string): Promise<void> {
	const res = await fetch(`${BASE}/api/artifacts/${encodeURIComponent(id)}`, {
		method: 'DELETE',
		headers: withAuthHeaders()
	});
	if (!res.ok && res.status !== 204) {
		const text = await res.text();
		throw new Error(`API error ${res.status}: ${text}`);
	}
}

export interface GitHubConnectorStatus {
	connected: boolean;
}

export async function getGitHubConnectorStatus(): Promise<GitHubConnectorStatus> {
	return request('/api/connectors/github/status');
}

export async function setGitHubPat(token: string): Promise<GitHubConnectorStatus> {
	return request('/api/connectors/github/pat', {
		method: 'POST',
		body: JSON.stringify({ token })
	});
}

export async function clearGitHubPat(): Promise<void> {
	const res = await fetch(`${BASE}/api/connectors/github/pat`, {
		method: 'DELETE',
		headers: withAuthHeaders()
	});
	if (!res.ok && res.status !== 204) {
		const text = await res.text();
		throw new Error(`API error ${res.status}: ${text}`);
	}
}

export interface ScheduledPostRow {
	content_id: string;
	platform: string | Record<string, unknown>;
	publish_at: string;
	status: string;
}

export async function getContentScheduled(
	sessionId: string,
	fromIso?: string,
	toIso?: string
): Promise<ScheduledPostRow[]> {
	const sp = new URLSearchParams({ session_id: sessionId });
	if (fromIso) sp.set('from', fromIso);
	if (toIso) sp.set('to', toIso);
	return request(`/api/dept/content/scheduled?${sp}`);
}

export async function getProfile(): Promise<unknown> {
	const res = await fetch(`${BASE}/api/profile`, { headers: withAuthHeaders() });
	return res.json();
}

export async function updateProfile(profile: unknown): Promise<unknown> {
	const res = await fetch(`${BASE}/api/profile`, {
		method: 'PUT',
		headers: withAuthHeaders(),
		body: JSON.stringify(profile)
	});
	return res.json();
}

// ── Knowledge (RAG) ───────────────────────────────────────────

export interface KnowledgeEntry {
	id: string;
	content: string;
	source: string;
	created_at: string;
	metadata: Record<string, unknown>;
}

export interface KnowledgeSearchResult {
	entry: KnowledgeEntry;
	score: number;
}

export interface KnowledgeStats {
	total_entries: number;
	model_name: string;
	dimensions: number;
}

export async function ingestKnowledge(
	content: string,
	source: string
): Promise<{ chunks_stored: number }> {
	return request('/api/knowledge/ingest', {
		method: 'POST',
		body: JSON.stringify({ content, source })
	});
}

export async function getKnowledge(): Promise<KnowledgeEntry[]> {
	return request('/api/knowledge');
}

export async function deleteKnowledge(id: string): Promise<void> {
	await fetch(`${BASE}/api/knowledge/${id}`, { method: 'DELETE', headers: withAuthHeaders() });
}

export async function searchKnowledge(
	query: string,
	limit?: number
): Promise<KnowledgeSearchResult[]> {
	return request('/api/knowledge/search', {
		method: 'POST',
		body: JSON.stringify({ query, limit: limit ?? 5 })
	});
}

export async function getKnowledgeStats(): Promise<KnowledgeStats> {
	return request('/api/knowledge/stats');
}

// ── Flow Engine (DAG) ─────────────────────────────────────────

export interface FlowConnectionDef {
	source_node: string;
	source_output?: string;
	target_node: string;
	target_input?: string;
}

export interface FlowNodeDef {
	id: string;
	node_type: string;
	name: string;
	parameters?: Record<string, unknown>;
	position?: [number, number];
	metadata?: Record<string, unknown>;
}

export interface FlowDef {
	id: string;
	name: string;
	description: string;
	nodes: FlowNodeDef[];
	connections: FlowConnectionDef[];
	variables?: Record<string, string>;
	metadata?: Record<string, unknown>;
}

/** Server treats nil UUID as “assign new id” on POST /api/flows. */
export const FLOW_CREATE_ID_PLACEHOLDER = '00000000-0000-0000-0000-000000000000';

/** Build a valid `FlowDef` for create from partial JSON or a visual editor draft. */
export function normalizeFlowForCreate(
	input: Partial<FlowDef> & Pick<FlowDef, 'name' | 'nodes' | 'connections'>
): FlowDef {
	const nodes = (input.nodes ?? []).map((n, i) => ({
		id: n.id?.trim() || crypto.randomUUID(),
		node_type: n.node_type?.trim() || 'agent',
		name: (n.name?.trim() || `Node ${i + 1}`) as string,
		parameters: n.parameters ?? {},
		position: n.position ?? [120 + (i % 5) * 48, 80 + Math.floor(i / 5) * 72],
		metadata: n.metadata ?? {}
	}));
	const connections = (input.connections ?? []).map((c) => ({
		source_node: c.source_node,
		target_node: c.target_node,
		source_output: c.source_output?.trim() || 'main',
		target_input: c.target_input?.trim() || 'main'
	}));
	return {
		id: input.id?.trim() || FLOW_CREATE_ID_PLACEHOLDER,
		name: input.name.trim() || 'Untitled flow',
		description: input.description?.trim() ?? '',
		nodes,
		connections,
		variables: input.variables ?? {},
		metadata: input.metadata ?? {}
	};
}

export type FlowNodeStatus = 'Pending' | 'Running' | 'Succeeded' | 'Failed' | 'Skipped';

export interface FlowNodeResult {
	status: FlowNodeStatus;
	output?: unknown;
	error?: string;
	started_at?: string | null;
	finished_at?: string | null;
}

export type FlowExecutionStatus = 'Queued' | 'Running' | 'Succeeded' | 'Failed' | 'Cancelled';

export interface FlowExecution {
	id: string;
	flow_id: string;
	status: FlowExecutionStatus;
	trigger_data: unknown;
	node_results: Record<string, FlowNodeResult>;
	started_at: string;
	finished_at?: string | null;
	error?: string | null;
	metadata?: Record<string, unknown>;
}

export async function getFlows(): Promise<FlowDef[]> {
	return request('/api/flows');
}

export async function createFlow(flow: Partial<FlowDef>): Promise<{ id: string }> {
	return request('/api/flows', { method: 'POST', body: JSON.stringify(flow) });
}

export async function deleteFlow(id: string): Promise<void> {
	const res = await fetch(`${BASE}/api/flows/${id}`, {
		method: 'DELETE',
		headers: withAuthHeaders()
	});
	if (!res.ok) {
		const text = await res.text();
		throw new Error(`API error ${res.status}: ${text}`);
	}
}

export async function runFlow(id: string): Promise<FlowExecution> {
	return request(`/api/flows/${id}/run`, {
		method: 'POST',
		body: JSON.stringify({ trigger_data: {} })
	});
}

export async function getFlowNodeTypes(): Promise<string[]> {
	return request('/api/flows/node-types');
}

/** Terminal pane for a flow execution (from `PaneSource::FlowNode`). */
export interface FlowTerminalPane {
	id: string;
	title: string;
	node_id?: string | null;
	flow_execution_id?: string | null;
	source?: { type?: string; value?: Record<string, unknown> };
}

export async function getFlowExecutionPanes(
	flowId: string,
	executionId: string
): Promise<FlowTerminalPane[]> {
	return request(`/api/flows/${flowId}/executions/${executionId}/panes`);
}

// ── Automations hub (native automation plane) ─────────────────

/** Cron list row (`GET /api/cron`) — summary without full payload. */
export interface CronScheduleSummary {
	id: string;
	name: string;
	session_id: string;
	schedule: string;
	enabled: boolean;
	event_kind: string;
	created_at?: string;
	last_fired_at?: string | null;
}

export async function getCronSchedulesList(): Promise<CronScheduleSummary[]> {
	const v = await request<unknown>('/api/cron');
	return Array.isArray(v) ? (v as CronScheduleSummary[]) : [];
}

export interface PlaybookListItem {
	id: string;
	name: string;
	description: string;
	category: string;
	steps?: unknown[];
}

export async function getPlaybooksList(): Promise<PlaybookListItem[]> {
	return request('/api/playbooks');
}

export interface PlaybookRunListItem {
	id: string;
	playbook_id: string;
	status: string;
	started_at?: string;
	completed_at?: string | null;
	error?: string | null;
}

export async function getPlaybookRunsList(): Promise<PlaybookRunListItem[]> {
	return request('/api/playbooks/runs');
}

export interface SecretListItem {
	key: string;
	label?: string | null;
	provider: string;
}

export async function listAutomationSecrets(): Promise<SecretListItem[]> {
	return request('/api/secrets');
}

export interface WebhookListItem {
	id: string;
	name: string;
	event_kind: string;
}

export async function getWebhooksList(): Promise<WebhookListItem[]> {
	const v = await request<unknown>('/api/webhooks');
	if (!Array.isArray(v)) return [];
	return v as WebhookListItem[];
}

// ── Visual Testing ────────────────────────────────────────────

export interface VisualIssue {
	type: string;
	description: string;
	element: string;
	suggested_fix: string;
}

export interface RouteAnalysis {
	route: string;
	severity: 'low' | 'medium' | 'high' | 'critical';
	issues: VisualIssue[];
	recommended_actions: { action_type: string; entity_description: string }[];
}

export interface VisualReport {
	run_id: string;
	timestamp: string;
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

export interface CorrectionResult {
	skills_created: string[];
	rules_created: string[];
	errors: string[];
}

export async function getVisualReports(): Promise<VisualReport[]> {
	return request('/api/system/visual-report');
}

export async function postVisualReport(report: VisualReport): Promise<{ id: string }> {
	return request('/api/system/visual-report', {
		method: 'POST',
		body: JSON.stringify(report)
	});
}

export async function triggerSelfCorrect(): Promise<CorrectionResult> {
	return request('/api/system/visual-report/self-correct', { method: 'POST' });
}

export async function runVisualTests(): Promise<{
	success: boolean;
	stdout: string;
	stderr: string;
}> {
	return request('/api/system/visual-test', { method: 'POST' });
}

// ── Analytics ─────────────────────────────────────────────────
export interface AnalyticsData {
	agents: number;
	skills: number;
	rules: number;
	mcp_servers: number;
	hooks: number;
	conversations: number;
	events: number;
	departments: number;
}

export async function getAnalytics(): Promise<AnalyticsData> {
	const res = await fetch(`${BASE}/api/analytics`, { headers: withAuthHeaders() });
	if (!res.ok) throw new Error('Failed to load analytics');
	return res.json();
}

/** S-047: aggregate counts + spend (same query params as spend). */
export interface AnalyticsDashboardResponse extends AnalyticsData {
	spend: AnalyticsSpendResponse;
}

export async function getAnalyticsDashboard(
	sessionId?: string | null,
	dept?: string
): Promise<AnalyticsDashboardResponse> {
	const sp = new URLSearchParams();
	if (sessionId) sp.set('session_id', sessionId);
	if (dept) sp.set('dept', dept);
	const q = sp.toString();
	return request<AnalyticsDashboardResponse>(`/api/analytics/dashboard${q ? `?${q}` : ''}`);
}
