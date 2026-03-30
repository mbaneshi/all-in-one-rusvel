<script lang="ts">
	import { tick } from 'svelte';
	import {
		SvelteFlow,
		Controls,
		Background,
		MiniMap,
		ConnectionLineType,
		type Node,
		type Edge,
		type NodeTypes,
		type Connection,
		addEdge
	} from '@xyflow/svelte';
	import '@xyflow/svelte/dist/style.css';
	import FlowCanvasNode from './FlowCanvasNode.svelte';
	import type { FlowConnectionDef, FlowNodeDef } from '$lib/api';

	let {
		nodeTypeOptions = [],
		flowNodes = $bindable([]),
		flowConnections = $bindable([])
	}: {
		nodeTypeOptions: string[];
		flowNodes: FlowNodeDef[];
		flowConnections: FlowConnectionDef[];
	} = $props();

	const nodeTypes: NodeTypes = {
		flowNode: FlowCanvasNode as unknown as NodeTypes[string]
	};

	let nodes: Node[] = $state([]);
	let edges: Edge[] = $state([]);
	let paletteType = $state('');

	function flowNodesToXY(n: FlowNodeDef[], onDelete: (id: string) => void): Node[] {
		return n.map((fn, i) => {
			const x = fn.position?.[0] ?? 80 + (i % 4) * 200;
			const y = fn.position?.[1] ?? 80 + Math.floor(i / 4) * 120;
			return {
				id: fn.id,
				type: 'flowNode',
				position: { x, y },
				data: {
					name: fn.name,
					nodeType: fn.node_type,
					onDelete
				}
			};
		});
	}

	function flowConnectionsToXY(c: FlowConnectionDef[]): Edge[] {
		return c.map((conn, i) => ({
			id: `e-${conn.source_node}-${conn.target_node}-${i}`,
			source: conn.source_node,
			target: conn.target_node,
			sourceHandle: conn.source_output && conn.source_output !== 'main' ? conn.source_output : 'main',
			targetHandle: conn.target_input && conn.target_input !== 'main' ? conn.target_input : undefined,
			animated: true
		}));
	}

	function syncFromFlow() {
		const onDel = removeNode;
		nodes = flowNodesToXY(flowNodes, onDel);
		edges = flowConnectionsToXY(flowConnections);
	}

	function xyToFlow(n: Node[], e: Edge[]): { nodes: FlowNodeDef[]; connections: FlowConnectionDef[] } {
		const outNodes: FlowNodeDef[] = n.map((node) => {
			const prev = flowNodes.find((x) => x.id === node.id);
			return {
				id: node.id,
				node_type: (node.data?.nodeType as string) ?? prev?.node_type ?? 'agent',
				name: (node.data?.name as string) ?? prev?.name ?? 'Node',
				parameters: prev?.parameters ?? {},
				position: [node.position.x, node.position.y],
				metadata: prev?.metadata ?? {}
			};
		});
		const outConn: FlowConnectionDef[] = e.map((edge) => ({
			source_node: edge.source,
			source_output: edge.sourceHandle === 'main' || !edge.sourceHandle ? 'main' : edge.sourceHandle,
			target_node: edge.target,
			target_input: edge.targetHandle === 'main' || !edge.targetHandle ? 'main' : edge.targetHandle
		}));
		return { nodes: outNodes, connections: outConn };
	}

	function pushFlowUpdate() {
		const { nodes: nn, connections: cc } = xyToFlow(nodes, edges);
		flowNodes = nn;
		flowConnections = cc;
	}

	function removeNode(nid: string) {
		nodes = nodes.filter((n) => n.id !== nid);
		edges = edges.filter((e) => e.source !== nid && e.target !== nid);
		pushFlowUpdate();
	}

	function addNodeOfType(nodeType: string) {
		if (!nodeType) return;
		const id = crypto.randomUUID();
		const idx = nodes.length;
		const n: Node = {
			id,
			type: 'flowNode',
			position: { x: 60 + (idx % 5) * 160, y: 40 + Math.floor(idx / 5) * 100 },
			data: {
				name: `${nodeType} ${idx + 1}`,
				nodeType,
				onDelete: removeNode
			}
		};
		nodes = [...nodes, n];
		pushFlowUpdate();
	}

	function onConnect(conn: Connection) {
		edges = addEdge(conn, edges);
		pushFlowUpdate();
	}

	function onNodeDragStop() {
		pushFlowUpdate();
	}

	async function onDelete() {
		await tick();
		pushFlowUpdate();
	}

	$effect(() => {
		void flowNodes;
		void flowConnections;
		syncFromFlow();
	});
</script>

<div class="flex min-h-[420px] flex-col gap-2 rounded-lg border border-border bg-background">
	<div class="flex flex-wrap items-center gap-2 border-b border-border px-2 py-2">
		<span class="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">Add node</span>
		<select
			bind:value={paletteType}
			class="h-8 max-w-[160px] rounded-md border border-border bg-secondary px-2 text-xs"
		>
			<option value="">Choose type…</option>
			{#each nodeTypeOptions as t}
				<option value={t}>{t}</option>
			{/each}
		</select>
		<button
			type="button"
			class="rounded-md bg-primary px-2 py-1 text-xs font-medium text-primary-foreground disabled:opacity-40"
			disabled={!paletteType}
			onclick={() => {
				addNodeOfType(paletteType);
				paletteType = '';
			}}
		>
			Add
		</button>
		<p class="text-[10px] text-muted-foreground">
			Drag nodes, connect handles (right → left). Edits sync to your flow draft.
		</p>
	</div>

	<div class="min-h-[380px] flex-1">
		<SvelteFlow
			bind:nodes
			bind:edges
			{nodeTypes}
			fitView
			connectionLineType={ConnectionLineType.SmoothStep}
			nodesConnectable={true}
			elementsSelectable={true}
			deleteKey="Delete"
			onconnect={onConnect}
			onnodedragstop={onNodeDragStop}
			ondelete={onDelete}
			proOptions={{ hideAttribution: true }}
		>
			<Controls class="!bg-card !border-border [&_button]:!border-border" />
			<Background gap={20} class="!bg-muted/20" />
			<MiniMap class="!bg-card !border-border" nodeStrokeWidth={2} />
		</SvelteFlow>
	</div>
</div>
