<script lang="ts">
	import { onMount } from 'svelte';
	import {
		getCronSchedulesList,
		getFlows,
		getPlaybookRunsList,
		getPlaybooksList,
		getRecentFlowExecutionsAcrossFlows,
		getWebhooksList,
		listAutomationSecrets,
		type CronScheduleSummary,
		type FlowDef,
		type FlowExecution,
		type PlaybookListItem,
		type PlaybookRunListItem,
		type SecretListItem,
		type WebhookListItem
	} from '$lib/api';
	import { activeSession } from '$lib/stores';
	import { GitBranch, Zap } from 'lucide-svelte';
	import { toast } from 'svelte-sonner';

	let loading = $state(true);
	let flows: FlowDef[] = $state([]);
	let playbooks: PlaybookListItem[] = $state([]);
	let cronRows: CronScheduleSummary[] = $state([]);
	let runs: PlaybookRunListItem[] = $state([]);
	let secrets: SecretListItem[] = $state([]);
	let webhooks: WebhookListItem[] = $state([]);
	let flowExecutions: FlowExecution[] = $state([]);
	let sessionId = $state<string | null>(null);

	activeSession.subscribe((s) => (sessionId = s?.id ?? null));

	async function load() {
		loading = true;
		try {
			const [f, p, c, r, sec, wh] = await Promise.all([
				getFlows(),
				getPlaybooksList(),
				getCronSchedulesList(),
				getPlaybookRunsList(),
				listAutomationSecrets(),
				getWebhooksList()
			]);
			flows = f;
			playbooks = p;
			cronRows = c;
			runs = r.slice(0, 20);
			secrets = sec;
			webhooks = wh;
			flowExecutions = await getRecentFlowExecutionsAcrossFlows(
				f.map((x) => x.id),
				24
			);
		} catch (e) {
			toast.error(e instanceof Error ? e.message : 'Failed to load automations');
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		void load();
	});

	const automationCronKind = 'rusvel.automation.v1';
	const automationWebhookKind = 'rusvel.automation.trigger';
	const secretPlaceholder = '{{secret:key}}';

	function isAutomationCron(row: CronScheduleSummary): boolean {
		return row.event_kind === automationCronKind;
	}

	function flowNameForExecution(flowId: string): string {
		const fl = flows.find((x) => x.id === flowId);
		return fl?.name ?? flowId.slice(0, 8);
	}
</script>

<div class="h-full overflow-auto p-6">
	<div class="mb-6 flex items-center gap-3">
		<Zap class="h-7 w-7 text-amber-500" strokeWidth={1.75} />
		<div>
			<h1 class="text-xl font-semibold text-foreground">Automations</h1>
			<p class="text-sm text-muted-foreground">
				Flows (DAG), playbooks, schedules, webhooks, secrets, and recent executions — one hub for the
				native automation plane.
			</p>
		</div>
		<button
			type="button"
			class="ml-auto rounded-md border border-border px-3 py-1.5 text-sm hover:bg-secondary"
			onclick={() => load()}
		>
			Refresh
		</button>
	</div>

	{#if sessionId}
		<p class="mb-4 text-xs text-muted-foreground">
			Active session: <span class="font-mono text-foreground">{sessionId}</span> — use
			<a href="/settings" class="text-primary underline">Settings</a> to switch.
		</p>
	{/if}

	{#if loading}
		<p class="text-sm text-muted-foreground">Loading…</p>
	{:else}
		<div class="grid gap-6 lg:grid-cols-2">
			<section class="rounded-xl border border-border bg-card p-4">
				<div class="mb-3 flex items-center gap-2">
					<GitBranch class="h-5 w-5 text-muted-foreground" strokeWidth={1.5} />
					<h2 class="text-sm font-semibold text-foreground">Flows (DAG)</h2>
					<a href="/flows" class="ml-auto text-sm text-primary hover:underline">Open editor →</a>
				</div>
				<p class="mb-2 text-xs text-muted-foreground">{flows.length} saved flow(s).</p>
				{#if flows.length === 0}
					<p class="text-sm text-muted-foreground">Create a flow on the Flows page.</p>
				{:else}
					<ul class="max-h-40 space-y-1 overflow-auto text-sm">
						{#each flows.slice(0, 12) as fl}
							<li class="truncate font-mono text-xs text-foreground">{fl.name}</li>
						{/each}
					</ul>
				{/if}
			</section>

			<section class="rounded-xl border border-border bg-card p-4">
				<h2 class="mb-3 text-sm font-semibold text-foreground">Playbooks</h2>
				<p class="mb-2 text-xs text-muted-foreground">
					Sequential steps (agent, flow, skill, rules) via
					<code class="rounded bg-muted px-1">GET /api/playbooks</code>.
				</p>
				{#if playbooks.length === 0}
					<p class="text-sm text-muted-foreground">No playbooks returned.</p>
				{:else}
					<ul class="max-h-40 space-y-1 overflow-auto text-sm">
						{#each playbooks as pb}
							<li>
								<span class="font-medium text-foreground">{pb.name}</span>
								<span class="text-xs text-muted-foreground"> · {pb.id}</span>
							</li>
						{/each}
					</ul>
				{/if}
			</section>

			<section class="rounded-xl border border-border bg-card p-4 lg:col-span-2">
				<div class="mb-3 flex items-center justify-between gap-2">
					<h2 class="text-sm font-semibold text-foreground">Schedules (cron)</h2>
					<a href="/tasks" class="text-sm text-primary hover:underline">Tasks page (jobs + cron)</a>
				</div>
				<p class="mb-2 text-xs text-muted-foreground">
					Rows with <code class="rounded bg-muted px-1">{automationCronKind}</code> run flows or
					playbooks via the job worker. Generic rows may only emit events.
				</p>
				{#if cronRows.length === 0}
					<p class="text-sm text-muted-foreground">No schedules. Use POST /api/cron to add one.</p>
				{:else}
					<div class="overflow-x-auto">
						<table class="w-full text-left text-xs">
							<thead>
								<tr class="border-b border-border text-muted-foreground">
									<th class="py-2 pr-2 font-medium">Name</th>
									<th class="py-2 pr-2 font-medium">Schedule</th>
									<th class="py-2 pr-2 font-medium">event_kind</th>
									<th class="py-2 pr-2 font-medium">On</th>
								</tr>
							</thead>
							<tbody>
								{#each cronRows as row}
									<tr class="border-b border-border/60 {isAutomationCron(row) ? 'bg-amber-500/5' : ''}">
										<td class="py-2 pr-2 text-foreground">{row.name}</td>
										<td class="py-2 pr-2 font-mono text-muted-foreground">{row.schedule}</td>
										<td class="py-2 pr-2 font-mono text-muted-foreground">{row.event_kind}</td>
										<td class="py-2 pr-2">{row.enabled ? 'yes' : 'no'}</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{/if}
			</section>

			<section class="rounded-xl border border-border bg-card p-4">
				<h2 class="mb-3 text-sm font-semibold text-foreground">Webhooks</h2>
				<p class="mb-2 text-xs text-muted-foreground">
					Use <code class="rounded bg-muted px-1">{automationWebhookKind}</code> to enqueue the
					same automation dispatch as cron.
				</p>
				{#if webhooks.length === 0}
					<p class="text-sm text-muted-foreground">None registered.</p>
				{:else}
					<ul class="space-y-1 text-sm">
						{#each webhooks as w}
							<li>
								<span class="text-foreground">{w.name}</span>
								<span class="font-mono text-xs text-muted-foreground"> · {w.event_kind}</span>
							</li>
						{/each}
					</ul>
				{/if}
			</section>

			<section class="rounded-xl border border-border bg-card p-4">
				<h2 class="mb-3 text-sm font-semibold text-foreground">Secrets</h2>
				<p class="mb-2 text-xs text-muted-foreground">
					Named credentials; use
					<code class="rounded bg-muted px-1">{secretPlaceholder}</code>
					in trigger variable strings. Manage under
					<a href="/settings" class="text-primary underline">Settings</a> or API.
				</p>
				{#if secrets.length === 0}
					<p class="text-sm text-muted-foreground">No secrets (keys only listed here).</p>
				{:else}
					<ul class="space-y-1 text-sm">
						{#each secrets as s}
							<li class="font-mono text-xs text-foreground">{s.key}</li>
						{/each}
					</ul>
				{/if}
			</section>

			<section class="rounded-xl border border-border bg-card p-4 lg:col-span-2">
				<div class="mb-3 flex flex-wrap items-center justify-between gap-2">
					<h2 class="text-sm font-semibold text-foreground">Flow executions (recent)</h2>
					<a href="/flows" class="text-sm text-primary hover:underline">Open Flows editor →</a>
				</div>
				<p class="mb-2 text-xs text-muted-foreground">
					Aggregated from <code class="rounded bg-muted px-1">GET /api/flows/:id/executions</code> per saved
					flow (newest first).
				</p>
				{#if flowExecutions.length === 0}
					<p class="text-sm text-muted-foreground">No executions yet. Run a flow from the Flows page.</p>
				{:else}
					<div class="overflow-x-auto">
						<table class="w-full text-left text-xs">
							<thead>
								<tr class="border-b border-border text-muted-foreground">
									<th class="py-2 pr-2 font-medium">Flow</th>
									<th class="py-2 pr-2 font-medium">Status</th>
									<th class="py-2 pr-2 font-medium">Started</th>
									<th class="py-2 pr-2 font-medium">Execution id</th>
								</tr>
							</thead>
							<tbody>
								{#each flowExecutions as ex}
									<tr class="border-b border-border/60">
										<td class="py-2 pr-2 text-foreground">{flowNameForExecution(ex.flow_id)}</td>
										<td class="py-2 pr-2">{ex.status}</td>
										<td class="py-2 pr-2 font-mono text-muted-foreground">{ex.started_at}</td>
										<td class="py-2 pr-2 font-mono text-muted-foreground">{ex.id}</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{/if}
			</section>

			<section class="rounded-xl border border-border bg-card p-4 lg:col-span-2">
				<h2 class="mb-3 text-sm font-semibold text-foreground">Playbook runs (recent)</h2>
				{#if runs.length === 0}
					<p class="text-sm text-muted-foreground">No runs yet.</p>
				{:else}
					<ul class="space-y-2 text-sm">
						{#each runs as run}
							<li class="rounded border border-border/80 px-2 py-1 font-mono text-xs">
								<span class="text-foreground">{run.playbook_id}</span>
								<span class="text-muted-foreground"> · {run.status} · {run.id}</span>
							</li>
						{/each}
					</ul>
				{/if}
			</section>
		</div>
	{/if}
</div>
