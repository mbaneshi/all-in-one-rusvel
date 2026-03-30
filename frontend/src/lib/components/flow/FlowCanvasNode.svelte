<script lang="ts">
	import { Handle, Position } from '@xyflow/svelte';

	let {
		id,
		data
	}: {
		id: string;
		data: {
			name: string;
			nodeType: string;
			onDelete?: (nid: string) => void;
		};
	} = $props();
</script>

<div
	class="min-w-[140px] max-w-[200px] rounded-lg border border-border bg-card px-2 py-1.5 shadow-sm"
>
	<Handle
		type="target"
		position={Position.Left}
		class="!h-2 !w-2 !border-2 !border-primary !bg-background"
	/>
	<div class="flex items-start justify-between gap-1">
		<div class="min-w-0 flex-1">
			<p class="truncate text-xs font-medium text-foreground">{data.name}</p>
			<p class="truncate font-mono text-[10px] text-muted-foreground">{data.nodeType}</p>
		</div>
		{#if data.onDelete}
			<button
				type="button"
				class="shrink-0 rounded px-1 text-[10px] text-muted-foreground hover:bg-destructive/15 hover:text-destructive"
				onclick={(e) => {
					e.stopPropagation();
					data.onDelete?.(id);
				}}
				title="Remove node"
			>
				×
			</button>
		{/if}
	</div>
	<Handle
		type="source"
		position={Position.Right}
		id="main"
		class="!h-2 !w-2 !border-2 !border-primary !bg-background"
	/>
</div>
