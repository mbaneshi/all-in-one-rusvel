<script lang="ts">
	import {
		checkHealth,
		getPendingApprovals,
		approveJob,
		rejectJob,
		getGitHubConnectorStatus,
		setGitHubPat,
		clearGitHubPat,
		getDepartments,
		type Job,
		type DepartmentDef
	} from '$lib/api';
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

	let departments = $state<DepartmentDef[]>([]);
	let deptsLoading = $state(true);

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

	async function loadDepartments() {
		deptsLoading = true;
		try {
			departments = await getDepartments();
		} catch (e) {
			toast.error(e instanceof Error ? e.message : 'Failed to load departments');
		} finally {
			deptsLoading = false;
		}
	}

	check();
	loadApprovals();
	loadGitHub();
	loadDepartments();
</script>

<div class="space-y-6">
	<div class="rounded-lg border border-dashed border-primary/30 bg-primary/5 px-4 py-3 text-sm text-foreground/90">
		<strong class="text-foreground">Models &amp; providers</strong> live under the
		<a href="/settings/llm" class="font-medium text-primary hover:underline">LLM &amp; models</a> tab.
		Spend charts are under
		<a href="/settings/spend" class="font-medium text-primary hover:underline">Spend</a>.
	</div>

	<!-- Departments directory -->
	<div class="rounded-xl border border-border bg-card p-5">
		<h2 class="mb-1 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
			Departments
		</h2>
		<p class="mb-4 text-xs text-muted-foreground">
			Per-department: Config, Chat, Agents, Skills, Rules, Events, Engine, and extras (Pipeline, CRM, …).
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
			Global views; department-scoped lists also live under each dept (Agents, Skills, Rules, MCP, Hooks,
			Flows).
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
					LLM routing is chosen at <strong>server boot</strong> from env. Default model ids and tool policy
					are under
					<a href="/settings/llm" class="text-primary hover:underline">LLM &amp; models</a>.
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
			Personal access token (stored on the Rusvel server). Injects context hints into department chat when
			set. Use fine-scoped PATs.
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
