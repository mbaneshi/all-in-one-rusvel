<script lang="ts">
	import { toast } from 'svelte-sonner';
	import { pendingCommand } from '$lib/stores';
	import { createSkill, deleteSkill, getSkills } from '$lib/api';
	import type { Skill } from '$lib/api';
	import type { DeptPanelColorClasses } from './DepartmentPanelAgentsTab.svelte';

	let {
		dept,
		cc,
		skills = $bindable<Skill[]>([])
	}: {
		dept: string;
		cc: DeptPanelColorClasses;
		skills: Skill[];
	} = $props();

	let showCreateSkill = $state(false);
	let newSkillName = $state('');
	let newSkillDesc = $state('');
	let newSkillPrompt = $state('');

	function sendQuickAction(prompt: string) {
		pendingCommand.set({ prompt });
	}

	async function refreshSkills() {
		try {
			skills = await getSkills(dept);
		} catch {
			skills = [];
		}
	}

	async function handleCreateSkill() {
		if (!newSkillName.trim()) return;
		try {
			await createSkill({
				id: '',
				name: newSkillName.trim(),
				description: newSkillDesc,
				prompt_template: newSkillPrompt,
				metadata: { engine: dept }
			});
			newSkillName = '';
			newSkillDesc = '';
			newSkillPrompt = '';
			showCreateSkill = false;
			await refreshSkills();
			toast.success('Skill created');
		} catch (e) {
			toast.error(`Failed to create skill: ${e instanceof Error ? e.message : e}`);
		}
	}

	async function handleDeleteSkill(id: string) {
		try {
			await deleteSkill(id);
			await refreshSkills();
			toast.success('Skill deleted');
		} catch (e) {
			toast.error(`Failed to delete skill: ${e instanceof Error ? e.message : e}`);
		}
	}
</script>

<div class="p-3 space-y-2">
	<button
		onclick={() => (showCreateSkill = !showCreateSkill)}
		class="w-full rounded-lg border border-dashed border-[var(--border)] py-1.5 text-xs text-[var(--muted-foreground)] {cc.hoverBorder} hover:text-[var(--foreground)]"
	>
		+ New Skill
	</button>
	{#if showCreateSkill}
		<div class="rounded-lg bg-[var(--secondary)] p-3 space-y-2">
			<input
				bind:value={newSkillName}
				placeholder="Skill name (e.g. /wire-engine)"
				class="w-full rounded-md border border-[var(--border)] bg-[var(--background)] px-2 py-1 text-xs text-[var(--foreground)] focus:outline-none"
			/>
			<input
				bind:value={newSkillDesc}
				placeholder="Description"
				class="w-full rounded-md border border-[var(--border)] bg-[var(--background)] px-2 py-1 text-xs text-[var(--foreground)] focus:outline-none"
			/>
			<textarea
				bind:value={newSkillPrompt}
				placeholder="Prompt template"
				rows="3"
				class="w-full rounded-md border border-[var(--border)] bg-[var(--background)] px-2 py-1 text-xs text-[var(--foreground)] focus:outline-none resize-none"
			></textarea>
			<button
				onclick={handleCreateSkill}
				class="w-full rounded-md {cc.button} py-1 text-xs font-medium text-white {cc.buttonHover}"
				>Create</button
			>
		</div>
	{/if}
	{#each skills as skill}
		<div
			class="rounded-lg bg-[var(--secondary)] p-2.5 transition-colors {cc.hoverBg} group cursor-pointer"
			role="button"
			tabindex="0"
			onclick={() => sendQuickAction('/' + skill.name.toLowerCase().replace(/ /g, '-'))}
			onkeydown={(e) => {
				if (e.key === 'Enter')
					sendQuickAction('/' + skill.name.toLowerCase().replace(/ /g, '-'));
			}}
		>
			<div class="flex items-center justify-between">
				<span class="text-xs font-mono font-medium {cc.text400}">{skill.name}</span>
				<button
					onclick={(e) => {
						e.stopPropagation();
						handleDeleteSkill(skill.id);
					}}
					class="hidden group-hover:block text-[var(--muted-foreground)] hover:text-danger-400 text-[10px]"
					>x</button
				>
			</div>
			<p class="text-[10px] text-[var(--muted-foreground)]">{skill.description}</p>
		</div>
	{/each}
	{#if skills.length === 0 && !showCreateSkill}
		<p class="text-center text-[10px] text-[var(--muted-foreground)] py-2">
			No skills. Create one above.
		</p>
	{/if}
</div>
