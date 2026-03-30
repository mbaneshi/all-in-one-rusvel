<script lang="ts">
	import { onMount } from 'svelte';
	import { getPendingApprovals, approveJob, rejectJob, type Job } from '$lib/api';
	import { refreshPendingApprovalCount } from '$lib/stores';
	import Button from '$lib/components/ui/Button.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import { toast } from 'svelte-sonner';
	import {
		humanizeJobKind,
		payloadSummaryRows,
		approvalPendingPreview,
		formatIsoDate
	} from '$lib/approvalContext';

	let jobs: Job[] = $state([]);
	let loading = $state(true);
	let busyId = $state<string | null>(null);
	let rejectNote: Record<string, string> = $state({});

	async function load() {
		loading = true;
		try {
			jobs = await getPendingApprovals();
			await refreshPendingApprovalCount();
		} catch (e) {
			toast.error(e instanceof Error ? e.message : 'Failed to load approvals');
			jobs = [];
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		load();
	});

	function formatPayload(payload: unknown): string {
		if (payload === null || payload === undefined) return '—';
		if (typeof payload === 'string') return payload;
		try {
			return JSON.stringify(payload, null, 2);
		} catch {
			return String(payload);
		}
	}

	function jobTitle(job: Job): string | null {
		const p = job.payload;
		if (p && typeof p === 'object' && p !== null && 'title' in p) {
			const t = (p as { title?: unknown }).title;
			if (typeof t === 'string' && t.trim()) return t;
		}
		return null;
	}

	async function approve(id: string) {
		busyId = id;
		try {
			await approveJob(id);
			jobs = jobs.filter((j) => j.id !== id);
			await refreshPendingApprovalCount();
			toast.success('Approved. The job will continue.');
		} catch (e) {
			toast.error(e instanceof Error ? e.message : 'Approve failed');
		} finally {
			busyId = null;
		}
	}

	async function reject(id: string) {
		busyId = id;
		try {
			const r = rejectNote[id]?.trim();
			await rejectJob(id, r || undefined);
			delete rejectNote[id];
			jobs = jobs.filter((j) => j.id !== id);
			await refreshPendingApprovalCount();
			toast.success('Rejected. The job was cancelled.');
		} catch (e) {
			toast.error(e instanceof Error ? e.message : 'Reject failed');
		} finally {
			busyId = null;
		}
	}
</script>

<div class="space-y-4">
	<div class="flex items-center justify-between gap-3">
		<p class="text-xs text-muted-foreground">
			Content publishing, outreach, and other gated work pauses here until you approve or reject.
		</p>
		<Button variant="secondary" size="sm" onclick={() => load()} disabled={loading}>
			{loading ? 'Refreshing…' : 'Refresh'}
		</Button>
	</div>

	{#if loading && jobs.length === 0}
		<div class="rounded-lg border border-border bg-card p-8 text-center text-sm text-muted-foreground">
			Loading queue…
		</div>
	{:else if jobs.length === 0}
		<EmptyState
			icon="check-circle"
			title="No pending approvals"
			description="When a job needs your sign-off, it will appear here and in department chat."
		/>
	{:else}
		<ul class="space-y-3">
			{#each jobs as job (job.id)}
				{@const title = jobTitle(job)}
				{@const kindLabel = humanizeJobKind(job.kind)}
				{@const rows = payloadSummaryRows(job.payload)}
				{@const preview = approvalPendingPreview(job.metadata)}
				{@const scheduled = formatIsoDate(job.scheduled_at)}
				<li
					class="rounded-lg border border-border bg-card p-4 shadow-sm"
				>
					<div class="flex flex-wrap items-start justify-between gap-3">
						<div class="min-w-0 flex-1">
							<div class="flex flex-wrap items-center gap-2">
								<span
									class="rounded-md bg-warning-500/15 px-2 py-0.5 text-[11px] font-medium text-warning-400"
								>
									{kindLabel}
								</span>
								<span class="font-mono text-[10px] text-muted-foreground">{job.id}</span>
							</div>
							{#if title}
								<p class="mt-1 text-sm font-medium text-foreground">{title}</p>
							{/if}
							<p class="mt-1 text-[11px] text-muted-foreground">
								Session <span class="font-mono">{job.session_id}</span>
								<span class="mx-1">·</span>
								{job.status}
								{#if scheduled}
									<span class="mx-1">·</span>
									Scheduled {scheduled}
								{/if}
							</p>
							{#if job.terminal_dept_id}
								<a
									href="/dept/{job.terminal_dept_id}/terminal"
									class="mt-1 inline-block text-[11px] text-primary underline underline-offset-2"
								>
									Open department terminal
									{#if job.terminal_pane_id}
										<span class="text-muted-foreground"> (pane linked)</span>
									{/if}
								</a>
							{/if}
							{#if job.terminal_context_snippet?.trim()}
								<details class="mt-2">
									<summary
										class="cursor-pointer text-[10px] font-medium text-muted-foreground hover:text-foreground"
									>
										Terminal context (scrollback snapshot)
									</summary>
									<pre
										class="mt-1 max-h-40 overflow-auto rounded-md border border-border bg-muted/30 p-2 font-mono text-[10px] leading-relaxed text-foreground whitespace-pre-wrap break-all"
									>{job.terminal_context_snippet}</pre>
								</details>
							{/if}
							{#if rows.length > 0}
								<dl
									class="mt-3 grid gap-x-4 gap-y-1 text-[11px] sm:grid-cols-[minmax(0,7rem)_1fr]"
								>
									{#each rows as r (r.label)}
										<dt class="text-muted-foreground">{r.label}</dt>
										<dd class="min-w-0 break-words font-mono text-foreground">{r.value}</dd>
									{/each}
								</dl>
							{/if}
							{#if preview}
								<div class="mt-3 rounded-md border border-border bg-muted/30 p-3">
									<p class="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
										{preview.headline}
									</p>
									<pre
										class="mt-2 max-h-48 overflow-auto whitespace-pre-wrap break-words font-sans text-xs leading-relaxed text-foreground"
									>{preview.body}</pre>
								</div>
							{/if}
						</div>
						<div class="flex shrink-0 flex-col items-end gap-2">
							<div class="flex gap-2">
								<Button
									variant="primary"
									size="sm"
									disabled={busyId !== null}
									loading={busyId === job.id}
									onclick={() => approve(job.id)}
								>
									Approve
								</Button>
								<Button
									variant="danger"
									size="sm"
									disabled={busyId !== null}
									loading={busyId === job.id}
									onclick={() => reject(job.id)}
								>
									Reject
								</Button>
							</div>
						</div>
					</div>
					<label class="sr-only" for="reject-{job.id}">Reject reason (optional)</label>
					<textarea
						id="reject-{job.id}"
						rows="2"
						placeholder="Optional reason if rejecting…"
						class="mt-2 w-full max-w-md rounded-md border border-border bg-secondary px-2 py-1.5 text-xs text-foreground placeholder:text-muted-foreground focus:border-primary focus:outline-none"
						value={rejectNote[job.id] ?? ''}
						oninput={(e) => {
							const v = (e.currentTarget as HTMLTextAreaElement).value;
							rejectNote = { ...rejectNote, [job.id]: v };
						}}
					></textarea>
					<details class="mt-3 group">
						<summary
							class="cursor-pointer list-none text-[11px] font-medium text-muted-foreground hover:text-foreground [&::-webkit-details-marker]:hidden"
						>
							<span class="underline underline-offset-2">Full payload and metadata</span>
							<span class="ml-1 text-muted-foreground/70 group-open:hidden">(expand)</span>
						</summary>
						<div class="mt-2 space-y-2">
							<div>
								<p class="text-[10px] font-medium text-muted-foreground">Payload</p>
								<pre
									class="mt-1 max-h-40 overflow-auto rounded-md bg-muted/40 p-3 font-mono text-[11px] leading-relaxed text-muted-foreground"
								>{formatPayload(job.payload)}</pre>
							</div>
							<div>
								<p class="text-[10px] font-medium text-muted-foreground">Metadata</p>
								<pre
									class="mt-1 max-h-32 overflow-auto rounded-md bg-muted/40 p-3 font-mono text-[11px] leading-relaxed text-muted-foreground"
								>{formatPayload(job.metadata)}</pre>
							</div>
						</div>
					</details>
				</li>
			{/each}
		</ul>
	{/if}
</div>
