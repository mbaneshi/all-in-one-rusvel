<script lang="ts">
	import { toast } from 'svelte-sonner';
	import {
		createWorkflow,
		deleteWorkflow,
		getWorkflows,
		runWorkflow
	} from '$lib/api';
	import type {
		Agent,
		Workflow,
		WorkflowStepDef,
		WorkflowRunResult
	} from '$lib/api';
	import type { DeptPanelColorClasses } from './DepartmentPanelAgentsTab.svelte';
	import WorkflowBuilder from '$lib/components/workflow/WorkflowBuilder.svelte';

	let {
		dept,
		cc,
		agents,
		workflows = $bindable<Workflow[]>([])
	}: {
		dept: string;
		cc: DeptPanelColorClasses;
		agents: Agent[];
		workflows: Workflow[];
	} = $props();

	let showCreateWorkflow = $state(false);
	let newWfName = $state('');
	let newWfDesc = $state('');
	let newWfSteps: WorkflowStepDef[] = $state([]);
	let runningWorkflowId: string | null = $state(null);
	let workflowResults: WorkflowRunResult | null = $state(null);

	async function refreshWorkflows() {
		try {
			workflows = await getWorkflows();
		} catch {
			workflows = [];
		}
	}

	async function handleCreateWorkflow() {
		if (!newWfName.trim() || newWfSteps.length === 0) return;
		try {
			await createWorkflow({
				name: newWfName.trim(),
				description: newWfDesc,
				steps: newWfSteps,
				metadata: { engine: dept }
			});
			newWfName = '';
			newWfDesc = '';
			newWfSteps = [];
			showCreateWorkflow = false;
			await refreshWorkflows();
			toast.success('Workflow created');
		} catch (e) {
			toast.error(`Failed to create workflow: ${e instanceof Error ? e.message : e}`);
		}
	}

	async function handleDeleteWorkflow(id: string) {
		try {
			await deleteWorkflow(id);
			await refreshWorkflows();
			toast.success('Workflow deleted');
		} catch (e) {
			toast.error(`Failed to delete workflow: ${e instanceof Error ? e.message : e}`);
		}
	}

	async function handleRunWorkflow(id: string) {
		runningWorkflowId = id;
		workflowResults = null;
		try {
			workflowResults = await runWorkflow(id);
			toast.success(`Workflow completed ($${workflowResults.total_cost_usd.toFixed(4)})`);
		} catch (e: unknown) {
			workflowResults = null;
			const msg = e instanceof Error ? e.message : String(e);
			toast.error(`Workflow failed: ${msg}`);
		} finally {
			runningWorkflowId = null;
		}
	}
</script>

<div class="p-3 space-y-2">
	<button
		onclick={() => (showCreateWorkflow = !showCreateWorkflow)}
		class="w-full rounded-lg border border-dashed border-[var(--border)] py-1.5 text-xs text-[var(--muted-foreground)] {cc.hoverBorder} hover:text-[var(--foreground)]"
	>
		+ New Workflow
	</button>
	{#if showCreateWorkflow}
		<div class="rounded-lg bg-secondary p-3 space-y-2">
			<input
				bind:value={newWfName}
				placeholder="Workflow name"
				class="w-full rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground focus:outline-none"
			/>
			<input
				bind:value={newWfDesc}
				placeholder="Description (optional)"
				class="w-full rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground focus:outline-none"
			/>

			<div class="border-t border-border pt-2">
				<p class="text-[10px] font-medium text-muted-foreground mb-1">
					Steps ({newWfSteps.length})
				</p>
				<WorkflowBuilder
					bind:steps={newWfSteps}
					agents={agents.map((a) => ({ name: a.name, role: a.role }))}
				/>
			</div>

			<button
				onclick={handleCreateWorkflow}
				disabled={!newWfName.trim() || newWfSteps.length === 0}
				class="w-full rounded-md bg-primary py-1 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-40 disabled:cursor-not-allowed"
				>Create Workflow</button
			>
		</div>
	{/if}
	{#each workflows as wf}
		<div class="rounded-lg bg-[var(--secondary)] p-2.5 group">
			<div class="flex items-center justify-between mb-1">
				<span class="text-xs font-medium text-[var(--foreground)]">{wf.name}</span>
				<div class="flex items-center gap-1">
					<span class="rounded {cc.badge} px-1.5 py-0.5 text-[9px] {cc.text400}"
						>{wf.steps.length} steps</span
					>
					<button
						onclick={() => handleDeleteWorkflow(wf.id)}
						class="hidden group-hover:block text-[var(--muted-foreground)] hover:text-danger-400 text-[10px]"
						>x</button
					>
				</div>
			</div>
			{#if wf.description}
				<p class="text-[10px] text-[var(--muted-foreground)] mb-1">{wf.description}</p>
			{/if}
			<div class="space-y-0.5 mb-2">
				{#each wf.steps as step, i}
					<div class="flex items-center gap-1 text-[9px] text-[var(--muted-foreground)]">
						<span class="text-[var(--muted-foreground)]">{i + 1}.</span>
						<span class="font-mono {cc.text400}">@{step.agent_name}</span>
						<span class="truncate"
							>{step.prompt_template.slice(0, 25)}{step.prompt_template.length > 25
								? '...'
								: ''}</span
						>
					</div>
				{/each}
			</div>
			<button
				onclick={() => handleRunWorkflow(wf.id)}
				disabled={runningWorkflowId === wf.id}
				class="w-full rounded-md {cc.buttonSemi} py-1 text-[10px] font-medium text-white {cc.buttonHover} disabled:opacity-50"
			>
				{runningWorkflowId === wf.id ? 'Running...' : 'Run Workflow'}
			</button>
		</div>
	{/each}
	{#if workflows.length === 0 && !showCreateWorkflow}
		<p class="text-center text-[10px] text-[var(--muted-foreground)] py-2">
			No workflows. Create one to chain agents together.
		</p>
	{/if}

	{#if workflowResults}
		<div class="mt-3 rounded-lg border {cc.borderLight} bg-[var(--secondary)] p-3 space-y-2">
			<div class="flex items-center justify-between">
				<span class="text-xs font-medium {cc.text300}"
					>Results: {workflowResults.workflow_name}</span
				>
				<span class="text-[9px] text-[var(--muted-foreground)]"
					>${workflowResults.total_cost_usd.toFixed(4)}</span
				>
			</div>
			{#each workflowResults.steps as result}
				<div class="rounded bg-[var(--card)] p-2">
					<div class="flex items-center gap-1 mb-1">
						<span class="text-[9px] text-[var(--muted-foreground)]"
							>Step {result.step_index + 1}</span
						>
						<span class="text-[10px] font-mono {cc.text400}">@{result.agent_name}</span>
						<span class="text-[9px] text-[var(--muted-foreground)] ml-auto"
							>${result.cost_usd.toFixed(4)}</span
						>
					</div>
					<p
						class="text-[10px] text-[var(--foreground)] whitespace-pre-wrap max-h-32 overflow-y-auto"
					>
						{result.output}
					</p>
				</div>
			{/each}
			<button
				onclick={() => (workflowResults = null)}
				class="w-full rounded-md bg-[var(--card)] py-1 text-[10px] text-[var(--muted-foreground)] hover:text-[var(--foreground)]"
				>Dismiss</button
			>
		</div>
	{/if}
</div>
