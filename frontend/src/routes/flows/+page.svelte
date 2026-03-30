<script lang="ts">
	import { onMount } from 'svelte';
	import {
		getFlows,
		createFlow,
		deleteFlow,
		runFlow,
		getFlowNodeTypes,
		getFlowExecutionPanes,
		normalizeFlowForCreate,
		type FlowDef,
		type FlowExecution,
		type FlowNodeDef,
		type FlowConnectionDef,
		type FlowTerminalPane
	} from '$lib/api';
	import FlowDagBuilder from '$lib/components/flow/FlowDagBuilder.svelte';
	import { toast } from 'svelte-sonner';
	import Button from '$lib/components/ui/Button.svelte';
	import DeptTerminal from '$lib/components/DeptTerminal.svelte';

	let flows: FlowDef[] = $state([]);
	let nodeTypes: string[] = $state([]);
	let loading = $state(true);
	let showCreate = $state(false);
	let createMode = $state<'visual' | 'json'>('visual');
	let newFlowJson = $state('');
	let visualName = $state('my-flow');
	let visualDescription = $state('');
	let visualNodes = $state<FlowNodeDef[]>([]);
	let visualConnections = $state<FlowConnectionDef[]>([]);
	let selectedExecution: FlowExecution | null = $state(null);
	let runningFlowId = $state('');
	let executionPanes: FlowTerminalPane[] = $state([]);
	let panesLoading = $state(false);

	function paneNodeLabel(pane: FlowTerminalPane, flow: FlowDef | undefined): string {
		const nid = pane.node_id ?? (pane.source?.value?.node_id as string | undefined);
		if (nid && flow) {
			const n = flow.nodes.find((x) => x.id === nid);
			if (n) return n.name;
		}
		return pane.title || pane.id.slice(0, 8);
	}

	onMount(async () => {
		try {
			[flows, nodeTypes] = await Promise.all([getFlows(), getFlowNodeTypes()]);
		} catch (e) {
			toast.error(e instanceof Error ? e.message : 'Failed to load flows');
		} finally {
			loading = false;
		}
	});

	function resetCreateForm() {
		newFlowJson = '';
		visualName = 'my-flow';
		visualDescription = '';
		visualNodes = [];
		visualConnections = [];
		createMode = 'visual';
	}

	async function handleCreate() {
		try {
			if (createMode === 'json') {
				const parsed = JSON.parse(newFlowJson || '{}') as Partial<FlowDef>;
				const body = normalizeFlowForCreate({
					name: parsed.name ?? 'Untitled flow',
					description: parsed.description,
					nodes: parsed.nodes ?? [],
					connections: parsed.connections ?? [],
					variables: parsed.variables,
					metadata: parsed.metadata,
					id: parsed.id
				});
				await createFlow(body);
			} else {
				if (!visualName.trim()) {
					toast.error('Enter a flow name');
					return;
				}
				const body = normalizeFlowForCreate({
					name: visualName.trim(),
					description: visualDescription,
					nodes: visualNodes,
					connections: visualConnections
				});
				await createFlow(body);
			}
			flows = await getFlows();
			showCreate = false;
			resetCreateForm();
			toast.success('Flow created');
		} catch (e) {
			toast.error(e instanceof Error ? e.message : 'Failed to create flow');
		}
	}

	async function handleDelete(id: string) {
		if (!confirm('Delete this flow?')) return;
		try {
			await deleteFlow(id);
			flows = flows.filter((f) => f.id !== id);
			toast.success('Flow deleted');
		} catch (e) {
			toast.error(e instanceof Error ? e.message : 'Failed to delete flow');
		}
	}

	async function handleRun(id: string) {
		runningFlowId = id;
		selectedExecution = null;
		executionPanes = [];
		try {
			const exec = await runFlow(id);
			selectedExecution = exec;
			toast.success(`Flow executed: ${exec.status}`);
			panesLoading = true;
			try {
				executionPanes = await getFlowExecutionPanes(id, exec.id);
			} catch {
				executionPanes = [];
			} finally {
				panesLoading = false;
			}
		} catch (e) {
			toast.error(e instanceof Error ? e.message : 'Flow execution failed');
		} finally {
			runningFlowId = '';
		}
	}

	function statusColor(status: string): string {
		switch (status) {
			case 'Succeeded':
				return 'text-green-500';
			case 'Failed':
				return 'text-red-500';
			case 'Running':
				return 'text-yellow-500';
			case 'Skipped':
				return 'text-gray-400';
			default:
				return 'text-muted-foreground';
		}
	}
</script>

<svelte:head>
	<title>Flows | RUSVEL</title>
</svelte:head>

<div class="mx-auto max-w-5xl space-y-6 p-6">
	<div class="flex items-center justify-between">
		<div>
			<h1 class="text-2xl font-bold">Flows</h1>
			<p class="text-sm text-muted-foreground">
				DAG workflows that chain department actions.
				{#if nodeTypes.length > 0}
					Node types: {nodeTypes.join(', ')}
				{/if}
			</p>
		</div>
		<Button
			onclick={() => {
				showCreate = !showCreate;
				if (showCreate) resetCreateForm();
			}}
		>
			{showCreate ? 'Cancel' : 'New Flow'}
		</Button>
	</div>

	{#if showCreate}
		<div class="rounded-lg border border-border bg-card p-4 space-y-4">
			<div class="flex flex-wrap items-center gap-2">
				<p class="text-sm font-medium">New flow</p>
				<div class="flex rounded-lg border border-border p-0.5 text-xs">
					<button
						type="button"
						class="rounded-md px-2 py-1 transition-colors {createMode === 'visual'
							? 'bg-primary text-primary-foreground'
							: 'text-muted-foreground hover:text-foreground'}"
						onclick={() => (createMode = 'visual')}
					>
						Visual builder
					</button>
					<button
						type="button"
						class="rounded-md px-2 py-1 transition-colors {createMode === 'json'
							? 'bg-primary text-primary-foreground'
							: 'text-muted-foreground hover:text-foreground'}"
						onclick={() => (createMode = 'json')}
					>
						JSON
					</button>
				</div>
			</div>

			{#if createMode === 'visual'}
				<div class="grid gap-3 sm:grid-cols-2">
					<label class="space-y-1">
						<span class="text-[10px] font-medium uppercase text-muted-foreground">Name</span>
						<input
							type="text"
							bind:value={visualName}
							class="w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm"
						/>
					</label>
					<label class="space-y-1 sm:col-span-2">
						<span class="text-[10px] font-medium uppercase text-muted-foreground">Description</span>
						<input
							type="text"
							bind:value={visualDescription}
							class="w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm"
							placeholder="Optional"
						/>
					</label>
				</div>
				<FlowDagBuilder
					nodeTypeOptions={nodeTypes.length > 0 ? nodeTypes : ['agent', 'code', 'condition']}
					bind:flowNodes={visualNodes}
					bind:flowConnections={visualConnections}
				/>
				<p class="text-[10px] text-muted-foreground">
					n8n-style canvas: add typed nodes, drag to arrange, connect right handle → left handle. Empty graph
					is allowed.
				</p>
			{:else}
				<p class="text-xs text-muted-foreground">
					Paste a partial definition — missing <code class="rounded bg-muted px-1">id</code> /
					<code class="rounded bg-muted px-1">metadata</code> are filled automatically. Use nil flow id for
					new flows.
				</p>
				<textarea
					bind:value={newFlowJson}
					rows="12"
					class="w-full rounded-md border border-border bg-background p-3 font-mono text-xs"
					placeholder={`{\n  "id": "00000000-0000-0000-0000-000000000000",\n  "name": "my-flow",\n  "description": "",\n  "nodes": [],\n  "connections": []\n}`}
				></textarea>
			{/if}
			<Button onclick={handleCreate}>Create</Button>
		</div>
	{/if}

	{#if loading}
		<p class="text-muted-foreground">Loading flows...</p>
	{:else if flows.length === 0}
		<div class="rounded-lg border border-dashed border-border p-8 text-center text-muted-foreground">
			<p>No flows yet. Create one to get started.</p>
		</div>
	{:else}
		<div class="space-y-3">
			{#each flows as flow}
				<div class="rounded-lg border border-border bg-card p-4">
					<div class="flex items-center justify-between">
						<div>
							<p class="font-medium">{flow.name}</p>
							<p class="text-xs text-muted-foreground">
								{flow.description || 'No description'}
								&mdash; {flow.nodes.length} nodes, {flow.connections.length} connections
							</p>
						</div>
						<div class="flex gap-2">
							<Button
								onclick={() => handleRun(flow.id)}
								disabled={runningFlowId === flow.id}
							>
								{runningFlowId === flow.id ? 'Running...' : 'Run'}
							</Button>
							<button
								type="button"
								onclick={() => handleDelete(flow.id)}
								class="rounded-md border border-border px-3 py-1.5 text-xs text-red-500 hover:bg-red-500/10"
							>
								Delete
							</button>
						</div>
					</div>

					<!-- Node list -->
					<div class="mt-3 flex flex-wrap gap-1.5">
						{#each flow.nodes as node}
							<span
								class="rounded-full bg-muted px-2 py-0.5 text-[10px] font-mono"
								title={`Type: ${node.node_type}`}
							>
								{node.name}
								<span class="text-muted-foreground">({node.node_type})</span>
							</span>
						{/each}
					</div>
				</div>
			{/each}
		</div>
	{/if}

	{#if selectedExecution}
		{@const exec = selectedExecution}
		<div class="rounded-lg border border-border bg-card p-4 space-y-3">
			<div class="flex items-center justify-between">
				<h2 class="font-medium">
					Execution
					<span class={statusColor(exec.status)}>
						{exec.status}
					</span>
				</h2>
				<button
					type="button"
					onclick={() => (selectedExecution = null)}
					class="text-xs text-muted-foreground hover:text-foreground"
				>
					Close
				</button>
			</div>

			{#if exec.error}
				<p class="text-sm text-red-500">{exec.error}</p>
			{/if}

			{#if panesLoading}
				<p class="text-xs text-muted-foreground">Loading terminal panes…</p>
			{:else if executionPanes.length > 0}
				{@const flowForExec = flows.find((f) => f.id === exec.flow_id)}
				<div class="grid gap-3 sm:grid-cols-2">
					{#each executionPanes as pane}
						<div class="flex min-h-[260px] flex-col gap-1">
							<p class="text-xs font-medium text-muted-foreground">
								{paneNodeLabel(pane, flowForExec)}
							</p>
							<div class="min-h-[240px] flex-1 overflow-hidden rounded-md border border-border">
								<DeptTerminal paneId={pane.id} />
							</div>
						</div>
					{/each}
				</div>
			{/if}

			<div class="space-y-2">
				{#each Object.entries(exec.node_results) as [nodeId, result]}
					<div
						class="rounded border border-border p-2 text-xs {result.status === 'Succeeded' ? 'border-green-500/30' : result.status === 'Failed' ? 'border-red-500/30' : result.status === 'Skipped' ? 'border-gray-500/20' : ''}"
					>
						<div class="flex justify-between">
							<span class="font-mono">{nodeId.slice(0, 8)}...</span>
							<span class={statusColor(result.status)}>{result.status}</span>
						</div>
						{#if result.output}
							<pre
								class="mt-1 max-h-32 overflow-auto rounded bg-muted/30 p-1 text-[9px] whitespace-pre-wrap"
							>{JSON.stringify(result.output, null, 2)}</pre>
						{/if}
						{#if result.error}
							<p class="mt-1 text-red-400">{result.error}</p>
						{/if}
					</div>
				{/each}
			</div>
		</div>
	{/if}
</div>
