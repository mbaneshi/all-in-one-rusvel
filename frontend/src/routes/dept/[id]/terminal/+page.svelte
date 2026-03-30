<script lang="ts">
	import { onMount } from 'svelte';
	import { get } from 'svelte/store';
	import DeptSectionScaffold from '$lib/components/department/DeptSectionScaffold.svelte';
	import DeptTerminalPane from '$lib/components/department/DeptTerminalPane.svelte';
	import { activeSession } from '$lib/stores';

	let currentSession: import('$lib/api').SessionSummary | null = $state(get(activeSession));

	onMount(() => {
		const unsub = activeSession.subscribe((v) => {
			currentSession = v;
		});
		return unsub;
	});
</script>

<DeptSectionScaffold>
	{#snippet children({ dept })}
		<!-- Match /terminal + BottomPanel: xterm needs a non-zero box; pure h-full in /dept/* often collapses to 0×0 so FitAddon skips PTY resize and the shell never redraws after WS attach. -->
		<div class="flex h-full w-full min-h-[320px] min-w-0 flex-1 flex-col">
			<DeptTerminalPane dept={dept.id} sessionId={currentSession?.id ?? null} />
		</div>
	{/snippet}
</DeptSectionScaffold>
