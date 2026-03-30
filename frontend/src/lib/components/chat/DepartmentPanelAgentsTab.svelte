<script lang="ts">
	import { toast } from 'svelte-sonner';
	import { createAgent, deleteAgent, getAgents } from '$lib/api';
	import type { Agent } from '$lib/api';

	export type DeptPanelColorClasses = {
		borderLight: string;
		bgSubtle: string;
		hoverBgSubtle: string;
		hoverBorder: string;
		button: string;
		buttonHover: string;
		badge: string;
		text300: string;
		text400: string;
	};

	let {
		dept,
		cc,
		agents = $bindable<Agent[]>([])
	}: {
		dept: string;
		cc: DeptPanelColorClasses;
		agents: Agent[];
	} = $props();

	let showCreateAgent = $state(false);
	let newAgentName = $state('');
	let newAgentRole = $state('');
	let newAgentModel = $state('sonnet');
	let newAgentInstructions = $state('');

	async function refreshAgents() {
		try {
			agents = await getAgents(dept);
		} catch {
			agents = [];
		}
	}

	async function handleCreateAgent() {
		if (!newAgentName.trim()) return;
		try {
			await createAgent({
				name: newAgentName.trim(),
				role: newAgentRole,
				model: newAgentModel,
				instructions: newAgentInstructions,
				metadata: { engine: dept }
			});
			newAgentName = '';
			newAgentRole = '';
			newAgentInstructions = '';
			showCreateAgent = false;
			await refreshAgents();
			toast.success('Agent created');
		} catch (e) {
			toast.error(`Failed to create agent: ${e instanceof Error ? e.message : e}`);
		}
	}

	async function handleDeleteAgent(id: string) {
		try {
			await deleteAgent(id);
			await refreshAgents();
			toast.success('Agent deleted');
		} catch (e) {
			toast.error(`Failed to delete agent: ${e instanceof Error ? e.message : e}`);
		}
	}
</script>

<div class="p-3 space-y-2">
	<button
		onclick={() => (showCreateAgent = !showCreateAgent)}
		class="w-full rounded-lg border border-dashed border-[var(--border)] py-1.5 text-xs text-[var(--muted-foreground)] {cc.hoverBorder} hover:text-[var(--foreground)]"
	>
		+ New Agent
	</button>
	{#if showCreateAgent}
		<div class="rounded-lg bg-[var(--secondary)] p-3 space-y-2">
			<input
				bind:value={newAgentName}
				placeholder="Agent name"
				class="w-full rounded-md border border-[var(--border)] bg-[var(--background)] px-2 py-1 text-xs text-[var(--foreground)] focus:outline-none"
			/>
			<input
				bind:value={newAgentRole}
				placeholder="Role description"
				class="w-full rounded-md border border-[var(--border)] bg-[var(--background)] px-2 py-1 text-xs text-[var(--foreground)] focus:outline-none"
			/>
			<select
				bind:value={newAgentModel}
				class="w-full rounded-md border border-[var(--border)] bg-[var(--background)] px-2 py-1 text-xs text-[var(--foreground)]"
			>
				<option value="sonnet">Sonnet</option>
				<option value="opus">Opus</option>
				<option value="haiku">Haiku</option>
			</select>
			<textarea
				bind:value={newAgentInstructions}
				placeholder="System prompt / instructions"
				rows="3"
				class="w-full rounded-md border border-[var(--border)] bg-[var(--background)] px-2 py-1 text-xs text-[var(--foreground)] focus:outline-none resize-none"
			></textarea>
			<button
				onclick={handleCreateAgent}
				class="w-full rounded-md {cc.button} py-1 text-xs font-medium text-white {cc.buttonHover}"
				>Create</button
			>
		</div>
	{/if}
	{#each agents as agent}
		<div class="rounded-lg bg-[var(--secondary)] p-2.5 group">
			<div class="flex items-center justify-between mb-1">
				<span class="text-xs font-medium text-[var(--foreground)]">{agent.name}</span>
				<div class="flex items-center gap-1">
					<span class="rounded {cc.badge} px-1.5 py-0.5 text-[9px] {cc.text400}"
						>{agent.default_model.model}</span
					>
					<button
						onclick={() => handleDeleteAgent(agent.id)}
						class="hidden group-hover:block text-[var(--muted-foreground)] hover:text-danger-400 text-[10px]"
						>x</button
					>
				</div>
			</div>
			<p class="text-[10px] text-[var(--muted-foreground)]">{agent.role}</p>
		</div>
	{/each}
	{#if agents.length === 0 && !showCreateAgent}
		<p class="text-center text-[10px] text-[var(--muted-foreground)] py-2">
			No agents. Create one above.
		</p>
	{/if}
</div>
