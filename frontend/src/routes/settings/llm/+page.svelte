<script lang="ts">
	import {
		getConfig,
		updateConfig,
		getModels,
		getTools,
		getLlmProviders,
		type ChatConfig,
		type ModelOption,
		type ToolOption,
		type LlmProvidersReport
	} from '$lib/api';
	import { invalidate } from '$lib/cache';
	import { toast } from 'svelte-sonner';

	let appConfig = $state<ChatConfig | null>(null);
	let models = $state<ModelOption[]>([]);
	let tools = $state<ToolOption[]>([]);
	let appConfigLoading = $state(true);
	let appConfigSaving = $state(false);
	let showToolToggles = $state(false);

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
			const [cfg, mdls, tls] = await Promise.all([getConfig(), getModels(), getTools()]);
			appConfig = cfg;
			syncBudgetTurnsFromConfig(cfg);
			models = mdls;
			tools = tls;
		} catch (e) {
			toast.error(e instanceof Error ? e.message : 'Failed to load app settings');
		} finally {
			appConfigLoading = false;
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
			toast.error('Max turns must be an integer');
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
		const disallowed = idx >= 0 ? d.filter((t) => t !== toolName) : [...d, toolName];
		appConfig = { ...appConfig, disallowed_tools: disallowed };
	}

	function toolEnabled(name: string): boolean {
		return !appConfig?.disallowed_tools.includes(name);
	}

	let selectedModelHelp = $derived(
		models.find((m) => m.value === appConfig?.model)?.description ?? ''
	);

	let ollamaRow = $derived(llmReport?.providers.find((p) => p.id === 'ollama') ?? null);

	loadAppConfig();
	loadLlmProviders();
</script>

<div class="space-y-6">
	<div>
		<h2 class="text-lg font-semibold text-foreground">LLM & models</h2>
		<p class="mt-1 text-sm text-muted-foreground leading-relaxed">
			Provider health, default model, and tool policy. Departments can override in
			<strong>Dept → Config</strong>.
		</p>
	</div>

	<div class="rounded-xl border border-amber-500/35 bg-amber-500/5 p-4">
		<h3 class="text-sm font-semibold text-amber-800 dark:text-amber-200">Ollama &amp; agent tools</h3>
		<p class="mt-2 text-xs text-foreground/90 leading-relaxed">
			Any model that appears under <strong>Live Ollama models</strong> below is already pulled on your
			machine (<code class="rounded bg-muted px-1">ollama pull &lt;name&gt;</code> if it is missing).
			Use <code class="rounded bg-muted px-1">ollama/&lt;name&gt;</code> in <strong>App defaults</strong>.
		</p>
		<p class="mt-2 text-xs text-foreground/90 leading-relaxed">
			<strong>Tool use</strong> (chat agent loop with read_file, bash, engine tools, etc.) is not wired for
			the Ollama HTTP adapter yet: requests do not send tool definitions to Ollama. For full tool + UI
			flows, use Claude, Cursor, or OpenAI. Ollama is fine for simple text generation and local
			embedding-capable models where the app only calls generate/stream without tools.
		</p>
	</div>

	<!-- LLM provider wiring + health -->
	<div class="rounded-xl border border-border bg-card p-5">
		<div class="mb-3 flex flex-wrap items-center justify-between gap-2">
			<div>
				<h3 class="text-sm font-semibold uppercase tracking-wider text-primary">LLM providers</h3>
				<p class="mt-0.5 text-xs text-muted-foreground leading-relaxed">
					Two rows for Claude: <strong>Anthropic API</strong> (key) vs
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

			{#if ollamaRow?.healthy === true && llmReport.ollama_models.length === 0}
				<p class="mt-3 text-xs text-muted-foreground">
					Ollama responded OK but returned no model tags — check <code class="rounded bg-muted px-1"
						>ollama list</code>.
				</p>
			{/if}

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
					<span class="mt-2 w-full text-[10px] uppercase tracking-wide text-muted-foreground">Cursor</span>
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
						Live Ollama models at {llmReport.ollama_host} (installed / available to the server)
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
			{:else if ollamaRow?.healthy === false}
				<p class="mt-4 text-xs text-muted-foreground">
					No Ollama model list: daemon unreachable or error. Start Ollama and refresh.
				</p>
			{/if}
		{/if}
	</div>

	<!-- App defaults (global LLM) -->
	<div class="rounded-xl border border-border bg-card p-5">
		<h3 class="mb-1 text-sm font-semibold uppercase tracking-wider text-primary">App defaults</h3>
		<p class="mb-4 text-xs text-muted-foreground leading-relaxed">
			<code class="rounded bg-muted px-1">PUT /api/config</code> — default model and tool policy for flows
			that use global chat config.
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
</div>
