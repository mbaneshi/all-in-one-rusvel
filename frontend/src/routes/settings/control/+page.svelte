<script lang="ts">
	import {
		getSystemRuntime,
		updateOperatorPrefs,
		postShutdown,
		postReexec,
		checkHealth,
		type SystemRuntimeResponse
	} from '$lib/api';
	import { toast } from 'svelte-sonner';

	let runtime = $state<SystemRuntimeResponse | null>(null);
	let loading = $state(true);
	let error = $state('');
	let prefsSaving = $state(false);
	let shutdownOpen = $state(false);
	let shutdownBusy = $state(false);
	let reexecOpen = $state(false);
	let reexecBusy = $state(false);

	/** UI mode for operator pref; maps to `force_claude_cli` on save */
	let prefMode = $state<'env' | 'cli' | 'api'>('env');

	let healthLabel = $state('—');

	async function load() {
		loading = true;
		error = '';
		try {
			runtime = await getSystemRuntime();
			const p = runtime.operator_prefs_stored.force_claude_cli;
			prefMode = p === true ? 'cli' : p === false ? 'api' : 'env';
		} catch (e) {
			runtime = null;
			error = e instanceof Error ? e.message : 'Failed to load runtime snapshot';
		} finally {
			loading = false;
		}
	}

	async function refreshHealth() {
		try {
			const h = await checkHealth();
			healthLabel = h.status === 'ok' ? 'OK' : h.status ?? 'Error';
		} catch {
			healthLabel = 'Unreachable';
		}
	}

	function prefDraftFromMode(): boolean | null {
		if (prefMode === 'env') return null;
		if (prefMode === 'cli') return true;
		return false;
	}

	async function savePrefs() {
		prefsSaving = true;
		try {
			const res = await updateOperatorPrefs({ force_claude_cli: prefDraftFromMode() });
			toast.success(res.message);
			await load();
		} catch (e) {
			toast.error(e instanceof Error ? e.message : 'Save failed');
		} finally {
			prefsSaving = false;
		}
	}

	async function confirmShutdown() {
		shutdownBusy = true;
		try {
			const res = await postShutdown();
			toast.message(res.message, { duration: 8000 });
			shutdownOpen = false;
		} catch (e) {
			toast.error(e instanceof Error ? e.message : 'Shutdown request failed');
		} finally {
			shutdownBusy = false;
		}
	}

	async function confirmReexec() {
		reexecBusy = true;
		try {
			const res = await postReexec();
			toast.message(res.message, { duration: 10000 });
			reexecOpen = false;
		} catch (e) {
			toast.error(e instanceof Error ? e.message : 'Re-exec request failed');
		} finally {
			reexecBusy = false;
		}
	}

	function subsystemLabel(k: string): string {
		const map: Record<string, string> = {
			embedding: 'Embedding',
			vector_store: 'Vector store',
			deploy: 'Deploy',
			terminal: 'Terminal',
			cdp: 'Browser (CDP)'
		};
		return map[k] ?? k;
	}

	load();
	refreshHealth();
</script>

<div class="space-y-6">
	<div class="flex flex-wrap items-center justify-between gap-3">
		<div>
			<h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
				Operator control center
			</h2>
			<p class="mt-1 max-w-2xl text-xs text-muted-foreground leading-relaxed">
				In-process effective configuration (no secret values). Env-based options and operator prefs that
				affect LLM bootstrap apply only after a full process restart.
			</p>
		</div>
		<button
			type="button"
			disabled={loading}
			onclick={() => void load()}
			class="rounded-md border border-border bg-secondary px-3 py-1.5 text-xs hover:bg-muted disabled:opacity-50"
		>
			{loading ? 'Loading…' : 'Refresh'}
		</button>
	</div>

	{#if loading && !runtime}
		<p class="text-sm text-muted-foreground">Loading runtime snapshot…</p>
	{:else if error}
		<p class="text-sm text-destructive">{error}</p>
	{:else if runtime}
		<div class="rounded-xl border border-border bg-card p-5">
			<h3 class="mb-3 text-xs font-semibold uppercase tracking-wider text-primary">Process &amp; HTTP</h3>
			<dl class="grid gap-2 text-sm sm:grid-cols-2">
				<div class="flex justify-between gap-2 border-b border-border/60 pb-2">
					<dt class="text-muted-foreground">Uptime</dt>
					<dd class="font-mono text-foreground">{runtime.process.uptime_seconds}s</dd>
				</div>
				<div class="flex justify-between gap-2 border-b border-border/60 pb-2">
					<dt class="text-muted-foreground">HTTP listen</dt>
					<dd class="break-all font-mono text-xs text-foreground">{runtime.process.http_listen}</dd>
				</div>
				<div class="flex justify-between gap-2 border-b border-border/60 pb-2 sm:col-span-2">
					<dt class="text-muted-foreground">Data directory</dt>
					<dd class="break-all text-right font-mono text-xs text-foreground">{runtime.process.data_dir}</dd>
				</div>
				<div class="flex justify-between gap-2 sm:col-span-2">
					<dt class="text-muted-foreground">API tokens</dt>
					<dd class="text-right text-foreground">
						Admin configured: <strong>{runtime.auth.admin_token_configured ? 'yes' : 'no'}</strong> · Read:
						<strong>{runtime.auth.read_token_configured ? 'yes' : 'no'}</strong>
					</dd>
				</div>
			</dl>
		</div>

		<div class="rounded-xl border border-border bg-card p-5">
			<div class="mb-3 flex flex-wrap items-start justify-between gap-2">
				<div>
					<h3 class="text-xs font-semibold uppercase tracking-wider text-primary">LLM (live)</h3>
					<p class="mt-1 text-xs text-muted-foreground">
						Full probes and app defaults:
						<a href="/settings/llm" class="text-primary hover:underline">LLM &amp; models</a>
					</p>
				</div>
			</div>
			<p class="text-sm text-foreground">
				<code class="rounded bg-muted px-1 font-mono text-xs">claude/…</code> →
				<strong
					>{runtime.llm.claude_effective_transport === 'cli' ? 'Claude CLI' : 'Anthropic API'}</strong
				>
			</p>
			<ul class="mt-2 space-y-1 text-xs text-muted-foreground">
				{#if runtime.llm.claude_cli_forced_by_env}
					<li>
						<span class="text-amber-700 dark:text-amber-300">Env</span>:
						<code class="rounded bg-muted px-0.5">RUSVEL_USE_CLAUDE_CLI</code> forces CLI.
					</li>
				{/if}
				{#if runtime.llm.claude_cli_forced_by_operator_pref}
					<li>
						<span class="text-emerald-700 dark:text-emerald-300">Stored pref</span> forced CLI at boot.
					</li>
				{/if}
			</ul>
		</div>

		<div class="rounded-xl border border-dashed border-border bg-muted/10 p-5">
			<h3 class="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
				Claude transport drift
			</h3>
			<dl class="grid gap-2 text-xs sm:grid-cols-2">
				<div>
					<dt class="text-muted-foreground">Effective in process (CLI)</dt>
					<dd class="font-medium text-foreground">{String(runtime.drift.claude.effective_cli)}</dd>
				</div>
				<div>
					<dt class="text-muted-foreground">Env suggests CLI</dt>
					<dd class="font-medium text-foreground">{String(runtime.drift.claude.env_suggests_cli)}</dd>
				</div>
				<div class="sm:col-span-2">
					<dt class="text-muted-foreground">Operator pref (loaded at boot)</dt>
					<dd class="font-medium text-foreground">
						{runtime.drift.claude.operator_pref_force_claude_cli === null
							? 'unset (env only)'
							: String(runtime.drift.claude.operator_pref_force_claude_cli)}
					</dd>
				</div>
			</dl>
		</div>

		<div class="rounded-xl border border-border bg-card p-5">
			<h3 class="mb-3 text-xs font-semibold uppercase tracking-wider text-primary">
				Operator preferences (persisted)
			</h3>
			<p class="mb-3 text-xs text-muted-foreground leading-relaxed">
				Saving updates the store immediately; the running LLM stack does not reload until you restart the
				Rusvel process.
			</p>
			<label class="mb-2 block text-xs font-medium text-foreground">Force Claude CLI for
				<code class="rounded bg-muted px-0.5 font-mono">claude/…</code></label>
			<select
				class="mb-3 w-full max-w-md rounded-md border border-border bg-background px-3 py-2 text-sm"
				bind:value={prefMode}
			>
				<option value="env">Follow environment only (default)</option>
				<option value="cli">Force CLI (like RUSVEL_USE_CLAUDE_CLI=1)</option>
				<option value="api">Prefer API when key present (force false)</option>
			</select>
			<button
				type="button"
				disabled={prefsSaving}
				onclick={() => void savePrefs()}
				class="rounded-md bg-primary px-3 py-2 text-xs text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
			>
				{prefsSaving ? 'Saving…' : 'Save operator prefs'}
			</button>
		</div>

		<div class="rounded-xl border border-border bg-card p-5">
			<h3 class="mb-3 text-xs font-semibold uppercase tracking-wider text-primary">Integrations</h3>
			<ul class="space-y-1 text-sm">
				<li class="flex justify-between gap-2">
					<span class="text-muted-foreground">Telegram / channel</span>
					<span>{runtime.integrations.telegram_channel_configured ? 'configured' : 'not wired'}</span>
				</li>
				<li class="flex justify-between gap-2">
					<span class="text-muted-foreground">SMTP host env</span>
					<span>{runtime.integrations.smtp_host_set ? 'set' : 'unset'}</span>
				</li>
				<li class="flex justify-between gap-2">
					<span class="text-muted-foreground">MCP HTTP auth env</span>
					<span>{runtime.integrations.mcp_http_auth_env ? 'on' : 'off'}</span>
				</li>
			</ul>
		</div>

		<div class="rounded-xl border border-border bg-card p-5">
			<h3 class="mb-3 text-xs font-semibold uppercase tracking-wider text-primary">Subsystems</h3>
			<ul class="flex flex-wrap gap-2">
				{#each Object.entries(runtime.subsystems) as [k, on] (k)}
					<li
						class="rounded-md border px-2 py-1 text-xs {on
							? 'border-emerald-500/40 bg-emerald-500/10 text-emerald-800 dark:text-emerald-200'
							: 'border-border bg-muted/30 text-muted-foreground'}"
					>
						{subsystemLabel(k)}: {on ? 'on' : 'off'}
					</li>
				{/each}
			</ul>
		</div>

		<div class="rounded-xl border border-border bg-card p-5">
			<div class="mb-3 flex flex-wrap items-center justify-between gap-2">
				<h3 class="text-xs font-semibold uppercase tracking-wider text-primary">Health</h3>
				<button
					type="button"
					onclick={() => void refreshHealth()}
					class="text-xs text-muted-foreground hover:text-foreground"
				>
					Refresh /api/health
				</button>
			</div>
			<p class="text-sm">
				<span class="text-muted-foreground">Health endpoint:</span>
				<strong class="ml-1">{healthLabel}</strong>
			</p>
			<p class="mt-2 text-sm">
				<span class="text-muted-foreground">Database (runtime check):</span>
				<span class="ml-1 font-mono text-xs">{runtime.health.database}</span>
			</p>
			{#if Array.isArray(runtime.health.departments_failed) && runtime.health.departments_failed.length > 0}
				<p class="mt-2 text-xs text-destructive">
					{runtime.health.departments_failed.length} department(s) failed at boot — see server logs.
				</p>
			{/if}
		</div>

		<div class="rounded-xl border border-border bg-card p-5">
			<h3 class="mb-3 text-xs font-semibold uppercase tracking-wider text-primary">Capabilities</h3>
			<ul class="grid gap-2 sm:grid-cols-2">
				{#each runtime.capabilities as c (c.id)}
					<li class="rounded-md border border-border bg-muted/10 px-3 py-2 text-sm">
						{#if c.settings_href}
							<a href={c.settings_href} class="font-medium text-primary hover:underline">{c.label}</a>
						{:else}
							<span class="font-medium text-foreground">{c.label}</span>
						{/if}
						<p class="mt-0.5 font-mono text-[10px] text-muted-foreground">{c.doc_path}</p>
					</li>
				{/each}
			</ul>
		</div>

		<div class="rounded-xl border border-amber-500/35 bg-amber-500/5 p-5">
			<h3 class="mb-2 text-xs font-semibold uppercase tracking-wider text-amber-800 dark:text-amber-200">
				Restart same process (re-exec)
			</h3>
			<p class="mb-3 text-xs text-muted-foreground leading-relaxed">
				Graceful shutdown, then replace this process with the same executable and arguments so operator prefs
				and env are picked up at boot. Enable
				<code class="rounded bg-muted px-0.5 font-mono text-[10px]">RUSVEL_ALLOW_REEXEC=1</code>
				on the server before starting Rusvel (Unix only). Can misbehave under
				<code class="rounded bg-muted px-0.5 font-mono text-[10px]">cargo run</code>; use Stop server + manual
				start or a process manager if unsure.
			</p>
			<dl class="mb-3 grid gap-2 text-xs sm:grid-cols-3">
				<div>
					<dt class="text-muted-foreground">Env flag</dt>
					<dd class="font-medium text-foreground">{runtime.reexec.env_enabled ? 'on' : 'off'}</dd>
				</div>
				<div>
					<dt class="text-muted-foreground">Unix host</dt>
					<dd class="font-medium text-foreground">{runtime.reexec.platform_unix ? 'yes' : 'no'}</dd>
				</div>
				<div>
					<dt class="text-muted-foreground">Re-exec offered</dt>
					<dd class="font-medium text-foreground">{runtime.reexec.available ? 'yes' : 'no'}</dd>
				</div>
			</dl>
			<button
				type="button"
				disabled={!runtime.reexec.available || reexecBusy}
				onclick={() => (reexecOpen = true)}
				class="rounded-md border border-amber-600/50 bg-amber-500/10 px-3 py-2 text-xs font-medium text-amber-900 dark:text-amber-100 hover:bg-amber-500/20 disabled:cursor-not-allowed disabled:opacity-40"
			>
				Re-exec process…
			</button>
		</div>

		<div class="rounded-xl border border-destructive/40 bg-destructive/5 p-5">
			<h3 class="mb-2 text-xs font-semibold uppercase tracking-wider text-destructive">Stop server</h3>
			<p class="mb-3 text-xs text-muted-foreground leading-relaxed">
				Requests graceful shutdown of this process. Restart from systemd, Docker, launchd, or your terminal.
				Requires an admin API token when <code class="rounded bg-muted px-0.5">RUSVEL_API_TOKEN</code> is set.
			</p>
			<button
				type="button"
				onclick={() => (shutdownOpen = true)}
				class="rounded-md border border-destructive/60 bg-destructive/10 px-3 py-2 text-xs font-medium text-destructive hover:bg-destructive/20"
			>
				Stop server…
			</button>
		</div>

		{#if shutdownOpen}
			<div
				class="fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-4 backdrop-blur-sm"
				role="presentation"
				onclick={(e) => e.target === e.currentTarget && !shutdownBusy && (shutdownOpen = false)}
			>
				<div
					class="max-w-md rounded-xl border border-border bg-card p-5 shadow-lg"
					role="dialog"
					aria-modal="true"
					aria-labelledby="shutdown-title"
				>
					<h4 id="shutdown-title" class="text-sm font-semibold text-foreground">Shut down Rusvel?</h4>
					<p class="mt-2 text-xs text-muted-foreground leading-relaxed">
						The API will stop accepting requests and the process will exit. Start it again from your
						supervisor or <code class="rounded bg-muted px-0.5">cargo run</code> / your binary.
					</p>
					<div class="mt-4 flex justify-end gap-2">
						<button
							type="button"
							disabled={shutdownBusy}
							onclick={() => (shutdownOpen = false)}
							class="rounded-md border border-border px-3 py-1.5 text-xs hover:bg-muted disabled:opacity-50"
						>
							Cancel
						</button>
						<button
							type="button"
							disabled={shutdownBusy}
							onclick={() => void confirmShutdown()}
							class="rounded-md bg-destructive px-3 py-1.5 text-xs text-destructive-foreground hover:bg-destructive/90 disabled:opacity-50"
						>
							{shutdownBusy ? 'Stopping…' : 'Confirm shutdown'}
						</button>
					</div>
				</div>
			</div>
		{/if}

		{#if reexecOpen}
			<div
				class="fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-4 backdrop-blur-sm"
				role="presentation"
				onclick={(e) => e.target === e.currentTarget && !reexecBusy && (reexecOpen = false)}
			>
				<div
					class="max-w-md rounded-xl border border-border bg-card p-5 shadow-lg"
					role="dialog"
					aria-modal="true"
					aria-labelledby="reexec-title"
				>
					<h4 id="reexec-title" class="text-sm font-semibold text-foreground">Re-exec Rusvel?</h4>
					<p class="mt-2 text-xs text-muted-foreground leading-relaxed">
						The HTTP server will shut down and this process will be replaced with the same argv. Save
						operator prefs first if needed. You need
						<code class="rounded bg-muted px-0.5">RUSVEL_ALLOW_REEXEC=1</code> and an admin API token when
						auth is enabled.
					</p>
					<div class="mt-4 flex justify-end gap-2">
						<button
							type="button"
							disabled={reexecBusy}
							onclick={() => (reexecOpen = false)}
							class="rounded-md border border-border px-3 py-1.5 text-xs hover:bg-muted disabled:opacity-50"
						>
							Cancel
						</button>
						<button
							type="button"
							disabled={reexecBusy}
							onclick={() => void confirmReexec()}
							class="rounded-md bg-amber-700 px-3 py-1.5 text-xs text-white hover:bg-amber-600 disabled:opacity-50 dark:bg-amber-800"
						>
							{reexecBusy ? 'Starting…' : 'Confirm re-exec'}
						</button>
					</div>
				</div>
			</div>
		{/if}

		<p class="text-[11px] text-muted-foreground leading-relaxed">
			{runtime.notes.restart ?? ''}
			{#if runtime.notes.shutdown}
				<span class="mt-1 block">{runtime.notes.shutdown}</span>
			{/if}
			{#if runtime.notes.reexec}
				<span class="mt-1 block">{runtime.notes.reexec}</span>
			{/if}
		</p>
	{/if}
</div>
