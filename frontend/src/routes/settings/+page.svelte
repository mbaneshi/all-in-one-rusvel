<script lang="ts">
	import {
		checkHealth,
		getPendingApprovals,
		approveJob,
		rejectJob,
		getGitHubConnectorStatus,
		setGitHubPat,
		clearGitHubPat,
		getConfig,
		updateConfig,
		getModels,
		getTools,
		getDepartments,
		getLlmProviders,
		type Job,
		type ChatConfig,
		type ModelOption,
		type ToolOption,
		type DepartmentDef,
		type LlmProvidersReport
	} from '$lib/api';
	import { invalidate } from '$lib/cache';
	import { refreshPendingApprovalCount } from '$lib/stores';
	import { toast } from 'svelte-sonner';

	let health = $state('checking...');
	let version = $state('0.1.0');

	let pendingJobs = $state<Job[]>([]);
	let approvalsLoading = $state(true);
	let approvalsError = $state('');
	let actionInFlight = $state<string | null>(null);

	let ghConnected = $state(false);
	let ghPat = $state('');
	let ghLoading = $state(true);

	let appConfig = $state<ChatConfig | null>(null);
	let models = $state<ModelOption[]>([]);
	let tools = $state<ToolOption[]>([]);
	let appConfigLoading = $state(true);
	let appConfigSaving = $state(false);
	let showToolToggles = $state(false);

	let departments = $state<DepartmentDef[]>([]);
	let deptsLoading = $state(true);

	let llmReport = $state<LlmProvidersReport | null>(null);
	let llmLoading = $state(false);

	const effortLevels = ['low', 'medium', 'high', 'max'] as const;
	const permissionOptions = [
		{ value: 'default', label: 'Default (CLI / agent default)' },
		{ value: 'plan', label: 'Plan' },
		{ value: 'supervised', label: 'Supervised (confirm tools)' },
		{ value: 'locked', label: 'Locked (no tools)' },
		{ value: 'auto', label: 'Auto (agent policy)' }
	];

	let maxBudgetStr = $state('');
	let maxTurnsStr = $state('');

	async function check() {
		try {
			const res = await checkHealth();
			health = res.status === 'ok' ? 'Connected' : 'Error';
		} catch {
			health = 'Disconnected';
		}
	}

	async function loadApprovals() {
		approvalsLoading = true;
		approvalsError = '';
		try {
			pendingJobs = await getPendingApprovals();
		} catch (e) {
			approvalsError = e instanceof Error ? e.message : 'Failed to load approvals';
		} finally {
			approvalsLoading = false;
		}
	}

	async function handleApprove(id: string) {
		actionInFlight = id;
		try {
			await approveJob(id);
			pendingJobs = pendingJobs.filter((j) => j.id !== id);
			await refreshPendingApprovalCount();
			toast.success('Job approved');
		} catch (e) {
			approvalsError = e instanceof Error ? e.message : 'Failed to approve job';
			toast.error(approvalsError);
		} finally {
			actionInFlight = null;
		}
	}

	async function handleReject(id: string) {
		actionInFlight = id;
		try {
			await rejectJob(id);
			pendingJobs = pendingJobs.filter((j) => j.id !== id);
			await refreshPendingApprovalCount();
			toast.success('Job rejected');
		} catch (e) {
			approvalsError = e instanceof Error ? e.message : 'Failed to reject job';
			toast.error(approvalsError);
		} finally {
			actionInFlight = null;
		}
	}

	function formatKind(kind: Job['kind']): string {
		if (typeof kind === 'string') return kind;
		if (typeof kind === 'object' && kind !== null) {
			const key = Object.keys(kind)[0];
			return key ?? JSON.stringify(kind);
		}
		return String(kind);
	}

	async function loadGitHub() {
		ghLoading = true;
		try {
			const s = await getGitHubConnectorStatus();
			ghConnected = s.connected;
		} catch {
			ghConnected = false;
		} finally {
			ghLoading = false;
		}
	}

	async function saveGitHubPat() {
		if (!ghPat.trim()) {
			toast.error('Paste a token first');
			return;
		}
		try {
			await setGitHubPat(ghPat.trim());
			ghPat = '';
			toast.success('GitHub token saved');
			await loadGitHub();
		} catch (e) {
			toast.error(e instanceof Error ? e.message : String(e));
		}
	}

	async function removeGitHubPat() {
		try {
			await clearGitHubPat();
			toast.success('GitHub token removed');
			await loadGitHub();
		} catch (e) {
			toast.error(e instanceof Error ? e.message : String(e));
		}
	}

	function syncBudgetTurnsFromConfig(c: ChatConfig) {
		maxBudgetStr =
			c.max_budget_usd != null && !Number.isNaN(c.max_budget_usd) ? String(c.max_budget_usd) : '';
		maxTurnsStr = c.max_turns != null ? String(c.max_turns) : '';
	}

	async function loadLlmProviders() {
		llmLoading = true;
		try {
			llmReport = await getLlmProviders();
		} catch (e) {
			llmReport = null;
			toast.error(e instanceof Error ? e.message : 'Failed to load LLM provider status');
		} finally {
			llmLoading = false;
		}
	}

	async function loadAppConfig() {
		appConfigLoading = true;
		try {
			const [cfg, mdls, tls, depts] = await Promise.all([
				getConfig(),
				getModels(),
				getTools(),
				getDepartments()
			]);
			appConfig = cfg;
			syncBudgetTurnsFromConfig(cfg);
			models = mdls;
			tools = tls;
			departments = depts;
		} catch (e) {
			toast.error(e instanceof Error ? e.message : 'Failed to load app settings');
		} finally {
			appConfigLoading = false;
			deptsLoading = false;
		}
	}

	function pickAppModel(value: string) {
		if (!appConfig) {
			toast.error('App config still loading');
			return;
		}
		appConfig = { ...appConfig, model: value };
		toast.message(`Model set to ${value} — press Save app defaults`, { duration: 4000 });
	}

	async function saveAppDefaults() {
		if (!appConfig) return;
		const budget = maxBudgetStr.trim();
		const turns = maxTurnsStr.trim();
		const next: ChatConfig = {
			...appConfig,
			max_budget_usd: budget === '' ? null : Number.parseFloat(budget),
			max_turns: turns === '' ? null : Number.parseInt(turns, 10)
		};
		if (next.max_budget_usd != null && Number.isNaN(next.max_budget_usd)) {
			toast.error('Max budget must be a number');
			return;
		}
		if (next.max_turns != null && Number.isNaN(next.max_turns)) {
			toast.error('Max turns must be a number');
			return;
		}
		appConfigSaving = true;
		try {
			appConfig = await updateConfig(next);
			syncBudgetTurnsFromConfig(appConfig);
			invalidate('global-config');
			toast.success('App defaults saved');
		} catch (e) {
			toast.error(e instanceof Error ? e.message : 'Save failed');
		} finally {
			appConfigSaving = false;
		}
	}

	function setModel(e: globalThis.Event) {
		const v = (e.target as HTMLSelectElement).value;
		if (appConfig) appConfig = { ...appConfig, model: v };
	}

	function setEffort(level: string) {
		if (appConfig) appConfig = { ...appConfig, effort: level };
	}

	function setPermissionMode(e: globalThis.Event) {
		const v = (e.target as HTMLSelectElement).value;
		if (appConfig) appConfig = { ...appConfig, permission_mode: v };
	}

	function toggleTool(toolName: string) {
		if (!appConfig) return;
		const d = appConfig.disallowed_tools;
		const idx = d.indexOf(toolName);
		const disallowed =
			idx >= 0 ? d.filter((t) => t !== toolName) : [...d, toolName];
		appConfig = { ...appConfig, disallowed_tools: disallowed };
	}

	function toolEnabled(name: string): boolean {
		return !appConfig?.disallowed_tools.includes(name);
	}

	let selectedModelHelp = $derived(
		models.find((m) => m.value === appConfig?.model)?.description ?? ''
	);

	check();
	loadApprovals();
	loadGitHub();
	loadAppConfig();
	loadLlmProviders();
</script>

<div class="p-6">
	<h1 class="mb-2 text-2xl font-bold text-foreground">Settings</h1>
	<p class="mb-6 text-sm text-muted-foreground">
		<a href="/settings/spend" class="text-primary hover:underline">LLM spend dashboard</a>
	</p>

	<div class="max-w-3xl space-y-6 pb-16">
		<!-- LLM provider wiring + health -->
		<div class="rounded-xl border border-border bg-card p-5">
			<div class="mb-3 flex flex-wrap items-center justify-between gap-2">
				<div>
					<h2 class="text-sm font-semibold uppercase tracking-wider text-primary">
						LLM providers
					</h2>
					<p class="mt-0.5 text-xs text-muted-foreground leading-relaxed">
						Two separate rows for Claude: <strong>Anthropic API</strong> (key) vs
						<strong>Claude CLI</strong> (<code class="rounded bg-muted px-0.5">claude -p</code>, like
						Cursor in the terminal). Only one backs <code class="rounded bg-muted px-0.5">claude/…</code>
						at boot — API if the key is set, otherwise CLI. Ollama/Cursor/OpenAI are probed where cheap.
					</p>
				</div>
				<button
					type="button"
					disabled={llmLoading}
					onclick={() => void loadLlmProviders()}
					class="shrink-0 rounded-md border border-border bg-secondary px-3 py-1.5 text-xs hover:bg-muted disabled:opacity-50"
				>
					{llmLoading ? 'Checking…' : 'Refresh status'}
				</button>
			</div>

			{#if llmLoading && !llmReport}
				<p class="text-sm text-muted-foreground">Loading provider probes…</p>
			{:else if !llmReport}
				<p class="text-sm text-destructive">No provider report (API unreachable).</p>
			{:else}
				<div class="grid gap-3 sm:grid-cols-2">
					{#each llmReport.providers as p (p.id)}
						<div class="rounded-lg border border-border bg-muted/15 p-3 text-sm">
							<div class="flex items-start justify-between gap-2">
								<span class="font-medium text-foreground">{p.display_name}</span>
								{#if p.healthy === true}
									<span class="shrink-0 text-xs font-medium text-emerald-500">OK</span>
								{:else if p.healthy === false}
									<span class="shrink-0 text-xs font-medium text-destructive">Fail</span>
								{:else}
									<span class="shrink-0 text-xs text-muted-foreground">n/a</span>
								{/if}
							</div>
							<p class="mt-1 break-all font-mono text-[10px] text-muted-foreground">{p.route}</p>
							{#if p.detail}
								<p class="mt-2 text-xs leading-relaxed text-foreground/85">{p.detail}</p>
							{/if}
						</div>
					{/each}
				</div>

				<div class="mt-4 border-t border-border pt-4">
					<p class="mb-2 text-xs font-medium text-muted-foreground">Quick model picks (then Save app defaults)</p>
					<div class="flex flex-wrap gap-2">
						<span class="w-full text-[10px] uppercase tracking-wide text-muted-foreground">
							Claude (<code class="font-mono">claude/…</code> → API or CLI per boot)
						</span>
						{#each ['claude/sonnet', 'claude/opus', 'claude/haiku'] as mid}
							<button
								type="button"
								class="rounded border border-border bg-background px-2 py-1 font-mono text-[10px] hover:bg-primary/10"
								onclick={() => pickAppModel(mid)}>{mid}</button>
						{/each}
						<span class="mt-2 w-full text-[10px] uppercase tracking-wide text-muted-foreground"
							>Cursor</span>
						{#each ['cursor/sonnet-4', 'cursor/gpt-5', 'cursor/sonnet-4-thinking'] as mid}
							<button
								type="button"
								class="rounded border border-border bg-background px-2 py-1 font-mono text-[10px] hover:bg-primary/10"
								onclick={() => pickAppModel(mid)}>{mid}</button>
						{/each}
					</div>
				</div>

				{#if llmReport.ollama_models.length > 0}
					<div class="mt-4">
						<p class="mb-2 text-xs font-medium text-muted-foreground">
							Live Ollama models at {llmReport.ollama_host}
						</p>
						<div class="max-h-40 overflow-y-auto rounded-md border border-border bg-background/50 p-2">
							<div class="flex flex-wrap gap-1.5">
								{#each llmReport.ollama_models as name (name)}
									<button
										type="button"
										title="Set App default to ollama/{name}"
										class="rounded border border-border px-2 py-0.5 font-mono text-[10px] hover:bg-chart-2/20"
										onclick={() => pickAppModel(`ollama/${name}`)}>ollama/{name}</button>
								{/each}
							</div>
						</div>
					</div>
				{/if}
			{/if}
		</div>

		<!-- App defaults (global LLM) -->
		<div class="rounded-xl border border-border bg-card p-5">
			<h2 class="mb-1 text-sm font-semibold uppercase tracking-wider text-primary">
				App defaults
			</h2>
			<p class="mb-4 text-xs text-muted-foreground leading-relaxed">
				<code class="rounded bg-muted px-1">PUT /api/config</code> — default model and tool policy for
				workspace flows that use global chat config. Each department can still override in
				<strong>Dept → Config</strong> (model, chat mode, budget).
			</p>

			{#if appConfigLoading}
				<p class="text-sm text-muted-foreground">Loading…</p>
			{:else if !appConfig}
				<p class="text-sm text-destructive">Could not load configuration.</p>
			{:else}
				<div class="space-y-4">
					<div>
						<label for="settings-model" class="mb-1 block text-xs font-medium text-muted-foreground"
							>Model</label
						>
						<select
							id="settings-model"
							value={appConfig.model}
							onchange={setModel}
							class="w-full max-w-xl rounded-md border border-border bg-secondary px-3 py-2 text-sm text-foreground"
						>
							{#each models as m}
								<option value={m.value} title={m.description}>{m.label}</option>
							{/each}
						</select>
						{#if selectedModelHelp}
							<p class="mt-1.5 text-xs text-muted-foreground leading-relaxed">{selectedModelHelp}</p>
						{/if}
					</div>

					<div>
						<span class="mb-1 block text-xs font-medium text-muted-foreground">Effort</span>
						<div class="flex flex-wrap gap-1 rounded-md border border-border bg-secondary p-0.5 w-fit">
							{#each effortLevels as level}
								<button
									type="button"
									onclick={() => setEffort(level)}
									class="rounded px-3 py-1.5 text-xs transition-colors {appConfig.effort === level
										? 'bg-primary text-primary-foreground'
										: 'text-muted-foreground hover:text-foreground'}"
								>
									{level}
								</button>
							{/each}
						</div>
					</div>

					<div>
						<label
							for="settings-perm"
							class="mb-1 block text-xs font-medium text-muted-foreground">Permission mode</label
						>
						<select
							id="settings-perm"
							value={appConfig.permission_mode}
							onchange={setPermissionMode}
							class="w-full max-w-xl rounded-md border border-border bg-secondary px-3 py-2 text-sm"
						>
							{#each permissionOptions as o}
								<option value={o.value}>{o.label}</option>
							{/each}
						</select>
					</div>

					<div class="grid gap-3 sm:grid-cols-2">
						<div>
							<label
								for="settings-budget"
								class="mb-1 block text-xs font-medium text-muted-foreground"
								>Max budget (USD)</label
							>
							<input
								id="settings-budget"
								type="text"
								inputmode="decimal"
								placeholder="optional"
								bind:value={maxBudgetStr}
								class="w-full rounded-md border border-border bg-background px-3 py-2 text-sm font-mono"
							/>
						</div>
						<div>
							<label
								for="settings-turns"
								class="mb-1 block text-xs font-medium text-muted-foreground">Max turns</label
							>
							<input
								id="settings-turns"
								type="text"
								inputmode="numeric"
								placeholder="optional"
								bind:value={maxTurnsStr}
								class="w-full rounded-md border border-border bg-background px-3 py-2 text-sm font-mono"
							/>
						</div>
					</div>

					<div>
						<button
							type="button"
							onclick={() => (showToolToggles = !showToolToggles)}
							class="mb-2 text-xs font-medium text-primary hover:underline"
						>
							{showToolToggles ? 'Hide' : 'Show'} tool allowlist (toggle disabled tools)
						</button>
						{#if showToolToggles}
							<div class="flex flex-wrap gap-2 rounded-md border border-border bg-muted/20 p-3">
								{#each tools as tool}
									<button
										type="button"
										title={tool.description}
										onclick={() => toggleTool(tool.name)}
										class="rounded-lg border px-2.5 py-1 text-xs transition-colors {toolEnabled(tool.name)
											? 'border-primary/50 bg-primary/10 text-foreground'
											: 'border-border text-muted-foreground line-through opacity-60'}"
									>
										{tool.name}
									</button>
								{/each}
							</div>
						{/if}
					</div>

					<button
						type="button"
						disabled={appConfigSaving}
						onclick={() => void saveAppDefaults()}
						class="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
					>
						{appConfigSaving ? 'Saving…' : 'Save app defaults'}
					</button>
				</div>
			{/if}
		</div>

		<!-- Departments directory -->
		<div class="rounded-xl border border-border bg-card p-5">
			<h2 class="mb-1 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
				Departments
			</h2>
			<p class="mb-4 text-xs text-muted-foreground">
				Per-department: Config, Chat, Agents, Skills, Rules, Events, Engine, and extras (Pipeline,
				CRM, …). Placeholder for future unified dept settings API.
			</p>
			{#if deptsLoading}
				<p class="text-sm text-muted-foreground">Loading…</p>
			{:else if departments.length === 0}
				<p class="text-sm text-muted-foreground">No departments from API.</p>
			{:else}
				<ul class="divide-y divide-border rounded-md border border-border">
					{#each departments as d (d.id)}
						<li class="flex flex-col gap-2 px-3 py-3 sm:flex-row sm:items-center sm:justify-between">
							<div>
								<p class="text-sm font-medium text-foreground">{d.title ?? d.name}</p>
								<p class="text-xs text-muted-foreground font-mono">{d.id}</p>
							</div>
							<div class="flex flex-wrap gap-2">
								<a
									href="/dept/{encodeURIComponent(d.id)}/config"
									class="rounded-md border border-border bg-secondary px-2 py-1 text-xs hover:bg-accent"
									>Config</a
								>
								<a
									href="/dept/{encodeURIComponent(d.id)}/chat"
									class="rounded-md border border-border bg-secondary px-2 py-1 text-xs hover:bg-accent"
									>Chat</a
								>
								<a
									href="/dept/{encodeURIComponent(d.id)}/agents"
									class="rounded-md border border-border bg-secondary px-2 py-1 text-xs hover:bg-accent"
									>Agents</a
								>
								<a
									href="/dept/{encodeURIComponent(d.id)}/skills"
									class="rounded-md border border-border bg-secondary px-2 py-1 text-xs hover:bg-accent"
									>Skills</a
								>
							</div>
						</li>
					{/each}
				</ul>
			{/if}
		</div>

		<!-- Workspace entities (cross-cutting) -->
		<div class="rounded-xl border border-border bg-card p-5">
			<h2 class="mb-1 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
				Workspace entities
			</h2>
			<p class="mb-3 text-xs text-muted-foreground">
				Global views; department-scoped lists also live under each dept (Agents, Skills, Rules, MCP,
				Hooks, Flows).
			</p>
			<div class="flex flex-wrap gap-2">
				<a
					href="/chat"
					class="rounded-md border border-dashed border-border px-3 py-1.5 text-xs hover:bg-muted/50"
					>God chat</a
				>
				<a
					href="/approvals"
					class="rounded-md border border-dashed border-border px-3 py-1.5 text-xs hover:bg-muted/50"
					>Approvals</a
				>
				<a
					href="/artifacts"
					class="rounded-md border border-dashed border-border px-3 py-1.5 text-xs hover:bg-muted/50"
					>Artifacts</a
				>
				<a
					href="/flows"
					class="rounded-md border border-dashed border-border px-3 py-1.5 text-xs hover:bg-muted/50"
					>Flows</a
				>
				<a
					href="/knowledge"
					class="rounded-md border border-dashed border-border px-3 py-1.5 text-xs hover:bg-muted/50"
					>Knowledge</a
				>
				<a
					href="/database/schema"
					class="rounded-md border border-dashed border-border px-3 py-1.5 text-xs hover:bg-muted/50"
					>Database</a
				>
				<a
					href="/tasks"
					class="rounded-md border border-dashed border-border px-3 py-1.5 text-xs hover:bg-muted/50"
					>Tasks</a
				>
			</div>
		</div>

		<!-- Approvals -->
		<div class="rounded-xl border border-amber-800/50 bg-card p-5">
			<div class="mb-4 flex items-center justify-between">
				<h3 class="text-sm font-semibold uppercase tracking-wider text-amber-600 dark:text-amber-400">
					Pending Approvals
				</h3>
				<button
					onclick={loadApprovals}
					class="rounded px-2 py-1 text-xs text-muted-foreground transition hover:bg-muted hover:text-foreground"
				>
					Refresh
				</button>
			</div>

			{#if approvalsLoading}
				<p class="text-sm text-muted-foreground">Loading...</p>
			{:else if approvalsError}
				<p class="text-sm text-destructive">{approvalsError}</p>
			{:else if pendingJobs.length === 0}
				<p class="text-sm text-muted-foreground">No jobs awaiting approval.</p>
			{:else}
				<div class="space-y-3">
					{#each pendingJobs as job (job.id)}
						<div class="rounded-lg border border-border bg-muted/20 p-3">
							<div class="mb-2 flex items-start justify-between">
								<div>
									<span
										class="inline-block rounded bg-amber-500/15 px-2 py-0.5 text-xs font-medium text-amber-700 dark:text-amber-300"
									>
										{formatKind(job.kind)}
									</span>
									<span class="ml-2 text-xs text-muted-foreground" title={job.id}>
										{job.id.slice(0, 8)}...
									</span>
								</div>
								<span class="text-xs text-muted-foreground">
									retries: {job.retries}/{job.max_retries}
								</span>
							</div>

							{#if job.payload && typeof job.payload === 'object'}
								<pre
									class="mb-3 max-h-24 overflow-auto rounded bg-background p-2 text-xs text-muted-foreground">{JSON.stringify(
										job.payload,
										null,
										2
									)}</pre>
							{/if}

							<div class="flex gap-2">
								<button
									onclick={() => handleApprove(job.id)}
									disabled={actionInFlight === job.id}
									class="rounded bg-green-700 px-3 py-1 text-xs font-medium text-green-100 transition hover:bg-green-600 disabled:opacity-50"
								>
									{actionInFlight === job.id ? 'Processing...' : 'Approve'}
								</button>
								<button
									onclick={() => handleReject(job.id)}
									disabled={actionInFlight === job.id}
									class="rounded bg-red-800 px-3 py-1 text-xs font-medium text-red-100 transition hover:bg-red-700 disabled:opacity-50"
								>
									{actionInFlight === job.id ? 'Processing...' : 'Reject'}
								</button>
							</div>
						</div>
					{/each}
				</div>
			{/if}
		</div>

		<div class="rounded-xl border border-border bg-card p-5">
			<h3 class="mb-4 text-sm font-semibold uppercase tracking-wider text-muted-foreground">System</h3>
			<div class="space-y-3 text-sm">
				<div class="flex flex-wrap items-center justify-between gap-2">
					<span class="text-muted-foreground">Version</span>
					<span class="text-foreground">{version}</span>
				</div>
				<div class="flex flex-wrap items-center justify-between gap-2">
					<span class="text-muted-foreground">API status</span>
					<span class={health === 'Connected' ? 'text-green-500' : 'text-destructive'}>{health}</span>
				</div>
				<div class="border-t border-border pt-3">
					<p class="text-xs text-muted-foreground leading-relaxed">
						LLM routing is chosen at <strong>server boot</strong> from env (e.g.
						<code class="rounded bg-muted px-1">ANTHROPIC_API_KEY</code>,
						<code class="rounded bg-muted px-1">OPENAI_API_KEY</code>, Ollama). Use
						<strong>App defaults</strong> above to pick which registered model id to call (e.g.
						<code class="rounded bg-muted px-1">ollama/…</code> to avoid paid APIs).
					</p>
				</div>
				<div class="flex flex-wrap items-center justify-between gap-2">
					<span class="text-muted-foreground">Database</span>
					<span class="text-right text-foreground">SQLite WAL (~/.rusvel/rusvel.db)</span>
				</div>
			</div>
		</div>

		<div class="rounded-xl border border-border bg-card p-5">
			<h3 class="mb-2 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
				GitHub connector
			</h3>
			<p class="mb-3 text-xs text-muted-foreground">
				Personal access token (stored on the Rusvel server). Injects context hints into department
				chat when set. Use fine-scoped PATs.
			</p>
			{#if ghLoading}
				<p class="text-sm text-muted-foreground">Loading…</p>
			{:else}
				<p class="mb-2 text-sm text-foreground">
					Status: {ghConnected ? 'Connected' : 'Not connected'}
				</p>
				<div class="flex flex-col gap-2 sm:flex-row sm:items-center">
					<input
						type="password"
						class="flex-1 rounded border border-border bg-background px-3 py-2 font-mono text-xs"
						placeholder="ghp_…"
						bind:value={ghPat}
					/>
					<button
						type="button"
						class="rounded bg-primary px-3 py-2 text-xs text-primary-foreground hover:bg-primary/90"
						onclick={() => saveGitHubPat()}
					>
						Save token
					</button>
					{#if ghConnected}
						<button
							type="button"
							class="rounded border border-border px-3 py-2 text-xs hover:bg-muted"
							onclick={() => removeGitHubPat()}
						>
							Remove
						</button>
					{/if}
				</div>
			{/if}
		</div>
	</div>
</div>
