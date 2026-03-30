<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { Terminal } from '@xterm/xterm';
	import { FitAddon } from '@xterm/addon-fit';
	import { terminalPaneResizeUrl, terminalWebSocketUrl } from '$lib/clientTerminalApi';
	import '@xterm/xterm/css/xterm.css';

	let {
		runId,
		apiOrigin = '',
		readOnly = false
	}: { runId: string; apiOrigin?: string; readOnly?: boolean } = $props();

	let termEl: HTMLDivElement;
	let term: Terminal | null = null;
	let fitAddon: FitAddon | null = null;
	let ws: WebSocket | null = null;
	let connected = $state(false);
	let error = $state('');
	let status = $state<'idle' | 'loading' | 'ready' | 'no_pane'>('idle');
	let activePaneId = $state<string | null>(null);
	let termReady = $state(false);
	let resizeTimer: ReturnType<typeof setTimeout> | undefined = undefined;

	function resolveHttpOrigin(): string {
		if (apiOrigin) return apiOrigin.replace(/\/$/, '');
		return '';
	}

	function postResize(rows: number, cols: number) {
		if (!activePaneId || rows <= 0 || cols <= 0) return;
		const url = apiOrigin
			? `${resolveHttpOrigin()}/api/terminal/pane/${encodeURIComponent(activePaneId)}/resize`
			: terminalPaneResizeUrl(activePaneId);
		fetch(url, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ rows, cols })
		}).catch(() => {});
	}

	function scheduleResizeNotify() {
		if (!term) return;
		if (resizeTimer !== undefined) clearTimeout(resizeTimer);
		resizeTimer = setTimeout(() => {
			resizeTimer = undefined;
			postResize(term!.cols, term!.rows);
		}, 80);
	}

	function getWsUrl(id: string): string {
		if (apiOrigin) {
			const base = resolveHttpOrigin();
			const u = new URL(base);
			const wsProto = u.protocol === 'https:' ? 'wss:' : 'ws:';
			const qp = new URLSearchParams({ pane_id: id });
			if (readOnly) qp.set('read_only', 'true');
			return `${wsProto}//${u.host}/api/terminal/ws?${qp.toString()}`;
		}
		return terminalWebSocketUrl(id, readOnly);
	}

	function disconnectWs() {
		ws?.close();
		ws = null;
		connected = false;
	}

	function connectWs(id: string) {
		error = '';
		disconnectWs();
		const url = getWsUrl(id);
		ws = new WebSocket(url);
		ws.binaryType = 'arraybuffer';

		ws.onopen = () => {
			connected = true;
			fitAddon?.fit();
			scheduleResizeNotify();
		};

		ws.onmessage = (ev: MessageEvent) => {
			if (ev.data instanceof ArrayBuffer) {
				term?.write(new Uint8Array(ev.data));
			} else if (typeof ev.data === 'string') {
				term?.write(ev.data);
			}
		};

		ws.onclose = () => {
			connected = false;
			term?.write('\r\n\x1b[90m[disconnected]\x1b[0m\r\n');
		};

		ws.onerror = () => {
			error = 'WebSocket connection failed';
			connected = false;
		};
	}

	type PaneRow = { id?: string };

	async function fetchFirstPaneIdForRun(rid: string): Promise<string | null> {
		const path = `/api/terminal/runs/${encodeURIComponent(rid)}/panes`;
		const url = apiOrigin ? `${resolveHttpOrigin()}${path}` : path;
		const r = await fetch(url);
		if (!r.ok) {
			error =
				r.status === 503 ? 'Terminal not configured on server' : 'Failed to list panes';
			return null;
		}
		error = '';
		const data = (await r.json()) as { panes?: PaneRow[] };
		const list = data.panes ?? [];
		const first = list[0]?.id;
		return typeof first === 'string' && first.length > 0 ? first : null;
	}

	async function attachToRun(rid: string) {
		if (!rid.trim()) {
			status = 'idle';
			activePaneId = null;
			disconnectWs();
			return;
		}
		status = 'loading';
		const id = await fetchFirstPaneIdForRun(rid);
		if (rid !== runId) return;
		if (!id) {
			status = 'no_pane';
			activePaneId = null;
			disconnectWs();
			return;
		}
		activePaneId = id;
		status = 'ready';
		term?.reset();
		connectWs(id);
	}

	onMount(() => {
		term = new Terminal({
			cursorBlink: false,
			disableStdin: true,
			fontSize: 13,
			fontFamily: 'ui-monospace, "Cascadia Code", "Fira Code", Menlo, monospace',
			theme: {
				background: '#09090b',
				foreground: '#fafafa',
				cursor: '#fafafa',
				selectionBackground: '#27272a',
				black: '#09090b',
				red: '#ef4444',
				green: '#22c55e',
				yellow: '#eab308',
				blue: '#3b82f6',
				magenta: '#a855f7',
				cyan: '#06b6d4',
				white: '#fafafa'
			}
		});

		fitAddon = new FitAddon();
		term.loadAddon(fitAddon);
		term.open(termEl);
		fitAddon.fit();
		termReady = true;

		const resizeObserver = new ResizeObserver(() => {
			fitAddon?.fit();
			scheduleResizeNotify();
		});
		resizeObserver.observe(termEl);

		return () => {
			if (resizeTimer !== undefined) clearTimeout(resizeTimer);
			resizeObserver.disconnect();
		};
	});

	$effect(() => {
		if (!termReady) return;
		const rid = runId;
		if (!rid.trim()) {
			status = 'idle';
			disconnectWs();
			return;
		}
		let cancelled = false;
		disconnectWs();
		void (async () => {
			await attachToRun(rid);
			if (cancelled || rid !== runId) return;
		})();
		return () => {
			cancelled = true;
		};
	});

	onDestroy(() => {
		disconnectWs();
		term?.dispose();
	});
</script>

<div class="flex h-full min-h-[240px] flex-col rounded-md border border-border bg-[#09090b]">
	<div
		class="flex items-center justify-between border-b border-border px-3 py-1.5 text-[10px] text-muted-foreground"
	>
		<span class="truncate font-mono" title={runId}>run {runId}</span>
		<span
			class="inline-flex items-center gap-1 rounded-full px-1.5 py-0.5 font-medium {connected
				? 'bg-green-500/10 text-green-500'
				: 'bg-muted text-muted-foreground'}"
		>
			<span class="h-1 w-1 rounded-full {connected ? 'bg-green-500' : 'bg-muted-foreground'}"></span>
			{status === 'loading'
				? 'Loading…'
				: status === 'no_pane'
					? 'Waiting for pane'
					: connected
						? 'Live'
						: 'Offline'}
		</span>
	</div>
	{#if error}
		<div class="border-b border-red-500/20 bg-red-500/5 px-3 py-1.5 text-[10px] text-red-500">
			{error}
		</div>
	{/if}
	<div class="flex min-h-0 flex-1 p-1" bind:this={termEl}></div>
</div>
