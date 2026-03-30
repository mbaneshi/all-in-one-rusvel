<script lang="ts">
	import Button from '$lib/components/ui/Button.svelte';
	import { payloadSummaryRows, toolNameToApprovalLabel } from '$lib/approvalContext';

	let {
		jobId,
		jobKind,
		payload,
		onApprove,
		onReject,
		terminalDeptId
	}: {
		jobId: string;
		jobKind: string;
		payload: Record<string, unknown>;
		onApprove: (jobId: string) => void;
		onReject: (jobId: string) => void;
		/** When set, show a link to the department terminal (ADR-008 context). */
		terminalDeptId?: string;
	} = $props();

	let decided = $state(false);
	let decision = $state<'approved' | 'rejected' | null>(null);

	const label = $derived(toolNameToApprovalLabel(jobKind));
	const rows = $derived(payloadSummaryRows(payload));

	function approve() {
		decided = true;
		decision = 'approved';
		onApprove(jobId);
	}

	function reject() {
		decided = true;
		decision = 'rejected';
		onReject(jobId);
	}
</script>

<div
	class="mt-2 rounded-md border border-warning-500/40 bg-warning-500/5 px-3 py-2.5 text-sm shadow-sm"
>
	<div class="flex items-center gap-2 text-xs font-medium text-warning-600 dark:text-warning-400">
		<span aria-hidden="true">&#x23F3;</span>
		<span>Awaiting approval · {label}</span>
	</div>
	{#if rows.length > 0}
		<dl class="mt-2 grid gap-x-3 gap-y-0.5 text-[11px] sm:grid-cols-[auto_1fr]">
			{#each rows as r (r.label)}
				<dt class="text-muted-foreground">{r.label}</dt>
				<dd class="min-w-0 break-words text-foreground">{r.value}</dd>
			{/each}
		</dl>
	{/if}
	{#if terminalDeptId}
		<a
			href="/dept/{terminalDeptId}/terminal"
			class="mt-2 inline-block text-[11px] text-primary underline underline-offset-2"
		>
			Open department terminal
		</a>
	{/if}
	<details class="mt-2 group">
		<summary
			class="cursor-pointer list-none text-[10px] text-muted-foreground hover:text-foreground [&::-webkit-details-marker]:hidden"
		>
			<span class="underline underline-offset-2">Raw arguments</span>
		</summary>
		<pre
			class="mt-1 max-h-28 overflow-auto rounded border border-border bg-muted/50 p-2 font-mono text-[10px] text-muted-foreground"
		>{JSON.stringify(payload, null, 2)}</pre>
	</details>
	{#if decided}
		<p
			class="mt-2 text-xs {decision === 'approved'
				? 'text-emerald-600 dark:text-emerald-400'
				: 'text-destructive'}"
		>
			{decision === 'approved' ? 'Approved' : 'Rejected'}
		</p>
	{:else}
		<div class="mt-2 flex flex-wrap gap-2">
			<Button variant="primary" size="sm" onclick={approve}>Approve</Button>
			<Button variant="danger" size="sm" onclick={reject}>Reject</Button>
		</div>
	{/if}
</div>
